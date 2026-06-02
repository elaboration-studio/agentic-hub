//! First-run scaffold: materialize a bundled demo shared-root tree at the
//! configured source root. The tree is embedded in the binary at build time
//! (`include_dir!`) so there is no network fetch and no missing-file failure
//! after install. See `docs/features/agentic-demo-scaffold.md`.

use std::path::{Component, Path, PathBuf};

use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// The bundled demo tree, embedded from `resources/agentic-demo/` at the repo
/// root (relative to this crate's manifest).
static DEMO: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../resources/agentic-demo");

/// How an existing file at a scaffold target is handled.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScaffoldMode {
    /// Write only missing paths; never overwrite an existing file.
    Merge,
    /// Replace bundled files even when present; never delete user-added files.
    Overwrite,
}

/// Summary of one scaffold run. Per-file failures are collected, never fatal.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldResult {
    /// Files newly created (target did not exist).
    pub written: u32,
    /// Files left untouched in `merge` mode because the target already existed.
    pub skipped: u32,
    /// Existing files replaced in `overwrite` mode.
    pub replaced: u32,
    /// Per-file failures (`<relative path>: <reason>`).
    pub errors: Vec<String>,
    /// The resolved destination root the tree was written into.
    pub destination_root: PathBuf,
}

/// Materialize the bundled demo tree at `dest_root`.
///
/// Refuses to run if `dest_root` is an existing non-directory. A missing
/// `dest_root` is created. Relative paths from the embedded tree are sanitized
/// (no `..`, no absolute components) before joining.
pub fn scaffold_demo(dest_root: &Path, mode: ScaffoldMode) -> Result<ScaffoldResult> {
    if dest_root.exists() && !dest_root.is_dir() {
        return Err(CoreError::NotADirectory(dest_root.to_path_buf()));
    }
    std::fs::create_dir_all(dest_root)?;

    let mut result = ScaffoldResult {
        destination_root: dest_root.to_path_buf(),
        ..Default::default()
    };

    let mut files: Vec<&include_dir::File<'_>> = Vec::new();
    collect_files(&DEMO, &mut files);

    for file in files {
        let rel = file.path();
        if !is_safe_relative(rel) {
            result
                .errors
                .push(format!("{}: unsafe path skipped", rel.display()));
            continue;
        }
        let target = dest_root.join(rel);
        match write_one(&target, file.contents(), mode) {
            Ok(Outcome::Written) => result.written += 1,
            Ok(Outcome::Replaced) => result.replaced += 1,
            Ok(Outcome::Skipped) => result.skipped += 1,
            Err(e) => result.errors.push(format!("{}: {e}", rel.display())),
        }
    }

    Ok(result)
}

enum Outcome {
    Written,
    Replaced,
    Skipped,
}

fn write_one(target: &Path, contents: &[u8], mode: ScaffoldMode) -> Result<Outcome> {
    let exists = target.exists();
    if exists && mode == ScaffoldMode::Merge {
        return Ok(Outcome::Skipped);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic per-file write: stage to `.tmp` then rename over the target.
    let tmp = tmp_sibling(target);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, target)?;
    Ok(if exists {
        Outcome::Replaced
    } else {
        Outcome::Written
    })
}

/// A `.tmp` sibling for atomic writes that never collides with the real file's
/// own extension (e.g. `SKILL.md` -> `SKILL.md.ah-tmp`).
fn tmp_sibling(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "scaffold".to_string());
    target.with_file_name(format!("{name}.ah-tmp"))
}

/// Reject any path with a root, prefix, or `..` component. The embedded tree is
/// trusted, but this keeps the join provably inside `dest_root`.
fn is_safe_relative(path: &Path) -> bool {
    path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Recursively collect every file under `dir` (include_dir only exposes the
/// immediate files / subdirs per level).
fn collect_files<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    for file in dir.files() {
        out.push(file);
    }
    for sub in dir.dirs() {
        collect_files(sub, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn merge_writes_into_empty_root_and_finds_demo_items() {
        let dir = tempfile::tempdir().unwrap();
        let result = scaffold_demo(dir.path(), ScaffoldMode::Merge).unwrap();
        assert!(result.written > 0);
        assert_eq!(result.skipped, 0);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        // The bundled tree's core directories and a demo skill must be present.
        assert!(dir.path().join("skills").is_dir());
        assert!(dir.path().join("agents").is_dir());
        assert!(dir.path().join("rules").is_dir());
        assert!(dir
            .path()
            .join("skills/agentic-hub/agentic-hub-setup/SKILL.md")
            .is_file());
    }

    #[test]
    fn merge_skips_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("README.md");
        fs::write(&readme, "MINE").unwrap();

        let result = scaffold_demo(dir.path(), ScaffoldMode::Merge).unwrap();
        assert!(result.skipped >= 1, "expected README.md skipped");
        assert_eq!(fs::read_to_string(&readme).unwrap(), "MINE");
    }

    #[test]
    fn overwrite_replaces_existing_but_keeps_user_files() {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("README.md");
        fs::write(&readme, "MINE").unwrap();
        let user_file = dir.path().join("my-notes.md");
        fs::write(&user_file, "KEEP").unwrap();

        let result = scaffold_demo(dir.path(), ScaffoldMode::Overwrite).unwrap();
        assert!(result.replaced >= 1, "expected README.md replaced");
        assert_ne!(fs::read_to_string(&readme).unwrap(), "MINE");
        // User-added files outside the bundled set are untouched.
        assert_eq!(fs::read_to_string(&user_file).unwrap(), "KEEP");
    }

    #[test]
    fn refuses_when_dest_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let as_file = dir.path().join("rootfile");
        fs::write(&as_file, "x").unwrap();
        let err = scaffold_demo(&as_file, ScaffoldMode::Merge).unwrap_err();
        assert!(matches!(err, CoreError::NotADirectory(_)));
    }

    #[test]
    fn no_tmp_artifacts_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_demo(dir.path(), ScaffoldMode::Merge).unwrap();
        let leftover: Vec<_> = walk(dir.path())
            .into_iter()
            .filter(|p| p.to_string_lossy().ends_with(".ah-tmp"))
            .collect();
        assert!(leftover.is_empty(), "tmp files left: {leftover:?}");
    }

    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(root).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
}
