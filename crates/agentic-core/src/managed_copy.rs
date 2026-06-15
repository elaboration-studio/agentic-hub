//! Managed-copy metadata, stored as a single per-root manifest.
//!
//! Some tools load capability files into memory at launch, so symlinks are
//! unreliable (Cursor agents, Claude skills). For those we write a real copy and
//! record it in a `.agentic-hub-managed.json` manifest at the tool's target root.
//! The manifest ties each copy back to its shared source for stale detection.
//!
//! On-disk shape and file name are identical to the VS Code extension's
//! `SymlinkPlanService` managed-copy manifest, for migration parity.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Per-root manifest file. One per target root (e.g. `~/.cursor/agents/`).
pub const MANIFEST_FILE: &str = ".agentic-hub-managed.json";

/// One managed-copy record, keyed in the manifest by the copy's path relative
/// to the target root (unix separators).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEntry {
    pub item_id: String,
    pub source_path: PathBuf,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    #[serde(default)]
    entries: BTreeMap<String, ManagedEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

/// Hex sha256 of arbitrary bytes.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Hex sha256 of a file's contents.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    Ok(hash_bytes(&std::fs::read(path)?))
}

/// Content hash for a managed-copy source or target: a skill folder hashes its
/// `SKILL.md`; a file hashes itself. `None` when the expected content is absent.
pub fn content_hash(path: &Path) -> Option<String> {
    if path.is_dir() {
        hash_file(&path.join("SKILL.md")).ok()
    } else {
        hash_file(path).ok()
    }
}

fn manifest_path(target_root: &Path) -> PathBuf {
    target_root.join(MANIFEST_FILE)
}

fn entry_key(target_root: &Path, target: &Path) -> String {
    target
        .strip_prefix(target_root)
        .unwrap_or(target)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Tolerant read: missing/malformed manifest → an empty manifest.
fn read_manifest(target_root: &Path) -> Manifest {
    std::fs::read_to_string(manifest_path(target_root))
        .ok()
        .and_then(|c| serde_json::from_str::<Manifest>(&c).ok())
        .filter(|m| m.version == 1)
        .unwrap_or_default()
}

fn write_manifest(target_root: &Path, manifest: &Manifest) -> std::io::Result<()> {
    let path = manifest_path(target_root);
    if manifest.entries.is_empty() {
        // Keep the root tidy when nothing is managed.
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::create_dir_all(target_root)?;
    std::fs::write(&path, serde_json::to_string_pretty(manifest)?)
}

/// The managed-copy record for `target`, if our manifest claims it.
pub fn read_entry(target_root: &Path, target: &Path) -> Option<ManagedEntry> {
    read_manifest(target_root)
        .entries
        .remove(&entry_key(target_root, target))
}

/// Copy `source` to `target` (a file copy, or a recursive folder copy for skill
/// directories) and record it in the root manifest. When `atomic`, file copies
/// stage to a sibling `.tmp` and rename into place.
pub fn write_managed_copy(
    source: &Path,
    target: &Path,
    target_root: &Path,
    item_id: &str,
    atomic: bool,
) -> std::io::Result<()> {
    use std::fs;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    if source.is_dir() {
        remove_existing(target)?;
        copy_dir(source, target)?;
    } else {
        let bytes = fs::read(source)?;
        if atomic {
            let tmp = target.with_extension("agentic.tmp");
            fs::write(&tmp, &bytes)?;
            fs::rename(&tmp, target)?;
        } else {
            fs::write(target, &bytes)?;
        }
    }

    let source_hash = content_hash(source).unwrap_or_default();
    let mut manifest = read_manifest(target_root);
    manifest.entries.insert(
        entry_key(target_root, target),
        ManagedEntry {
            item_id: item_id.to_string(),
            source_path: source.to_path_buf(),
            source_hash,
        },
    );
    write_manifest(target_root, &manifest)
}

/// Write rendered `content` to `target` and record it in the root manifest
/// against `source` (the original file, for stale detection). Used for managed
/// copies whose bytes are transformed from the source rather than copied
/// verbatim (Codex subagent TOML). When `atomic`, stages to a sibling `.tmp`
/// and renames into place.
pub fn write_managed_content(
    content: &[u8],
    target: &Path,
    target_root: &Path,
    item_id: &str,
    source: &Path,
    atomic: bool,
) -> std::io::Result<()> {
    use std::fs;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if atomic {
        let tmp = target.with_extension("agentic.tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, target)?;
    } else {
        fs::write(target, content)?;
    }

    let source_hash = content_hash(source).unwrap_or_default();
    let mut manifest = read_manifest(target_root);
    manifest.entries.insert(
        entry_key(target_root, target),
        ManagedEntry {
            item_id: item_id.to_string(),
            source_path: source.to_path_buf(),
            source_hash,
        },
    );
    write_manifest(target_root, &manifest)
}

/// Remove a managed copy (file or folder) and drop its manifest entry.
pub fn remove_managed_copy(target: &Path, target_root: &Path) -> std::io::Result<()> {
    remove_existing(target)?;
    let mut manifest = read_manifest(target_root);
    manifest.entries.remove(&entry_key(target_root, target));
    write_manifest(target_root, &manifest)
}

/// Remove managed copies recorded for `item_id` at any path other than `keep`,
/// deleting both the file/folder and its manifest entry, then sweeping any
/// now-empty parent directories under `target_root`. This self-heals a layout
/// or path change for a single item (e.g. a nested → flat agent move): the new
/// copy is written at `keep` first, then stale copies of the *same item*
/// elsewhere in the root are pruned. Only our own manifest-tracked copies are
/// touched — a user's own files are never affected.
pub fn prune_other_paths_for_item(
    target_root: &Path,
    item_id: &str,
    keep: &Path,
) -> std::io::Result<()> {
    let keep_key = entry_key(target_root, keep);
    let mut manifest = read_manifest(target_root);
    let stale: Vec<String> = manifest
        .entries
        .iter()
        .filter(|(key, entry)| entry.item_id == item_id && **key != keep_key)
        .map(|(key, _)| key.clone())
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    for key in &stale {
        let path = target_root.join(key);
        remove_existing(&path)?;
        remove_empty_ancestors(target_root, &path);
        manifest.entries.remove(key);
    }
    write_manifest(target_root, &manifest)
}

/// Remove now-empty directories from `child`'s parent up to (but not including)
/// `root`. Best-effort: stops at the first directory that is non-empty, equal
/// to `root`, outside `root`, or already gone.
fn remove_empty_ancestors(root: &Path, child: &Path) {
    let mut dir = child.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            break;
        }
        // `remove_dir` only succeeds on an empty directory; a failure means it
        // is non-empty (or gone), so stop climbing.
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Remove whatever sits at `target` — symlink, file, or directory — ignoring a
/// missing path. Used both for managed-copy refresh and for confirmed
/// `foreign_file` take-overs in the applier.
pub(crate) fn remove_existing(target: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(target) {
        Ok(m) if m.file_type().is_symlink() => std::fs::remove_file(target),
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(target),
        Ok(_) => std::fs::remove_file(target),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Current UTC time as an RFC 3339 / ISO 8601 string (`2026-05-31T08:30:00Z`).
/// Dependency-free civil-from-days conversion (Howard Hinnant's algorithm).
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // days since 1970-01-01 -> civil (y, m, d)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_copy_roundtrip_via_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        let source = dir.path().join("src/agent.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "agent v1").unwrap();
        let target = root.join("agent.md");

        write_managed_copy(&source, &target, &root, "agent:agent.md", false).unwrap();
        assert!(target.is_file());
        let entry = read_entry(&root, &target).unwrap();
        assert_eq!(entry.item_id, "agent:agent.md");
        assert_eq!(entry.source_path, source);
        assert_eq!(entry.source_hash, content_hash(&source).unwrap());

        remove_managed_copy(&target, &root).unwrap();
        assert!(!target.exists());
        assert!(read_entry(&root, &target).is_none());
        // Manifest deleted once empty.
        assert!(!root.join(MANIFEST_FILE).exists());
    }

    #[test]
    fn skill_dir_copy_records_skill_md_hash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        let source = dir.path().join("src/dev/tdd");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# tdd").unwrap();
        std::fs::write(source.join("ref.md"), "extra").unwrap();
        let target = root.join("dev/tdd");

        write_managed_copy(&source, &target, &root, "skill:dev/tdd", false).unwrap();
        assert!(target.join("SKILL.md").is_file());
        assert!(target.join("ref.md").is_file());
        let entry = read_entry(&root, &target).unwrap();
        assert_eq!(
            entry.source_hash,
            hash_file(&source.join("SKILL.md")).unwrap()
        );
    }

    #[test]
    fn managed_content_writes_rendered_bytes_and_tracks_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("codex-agents");
        let source = dir.path().join("src/cto.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "---\nname: cto\n---\nbody").unwrap();
        let target = root.join("cto.toml");

        write_managed_content(b"name = \"cto\"\n", &target, &root, "agent:cto.md", &source, true)
            .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "name = \"cto\"\n");
        assert!(!target.with_extension("agentic.tmp").exists());
        let entry = read_entry(&root, &target).unwrap();
        // The manifest tracks the markdown source (for stale detection), even
        // though the bytes on disk are the rendered TOML.
        assert_eq!(entry.source_path, source);
        assert_eq!(entry.source_hash, content_hash(&source).unwrap());
    }

    #[test]
    fn prune_removes_same_item_at_other_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        let src = dir.path().join("src/zoom/cto.md");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, "cto v1").unwrap();

        // Old nested managed copy (pre-flatten layout) + new flat copy for the
        // same item.
        let nested = root.join("zoom/cto.md");
        write_managed_copy(&src, &nested, &root, "agent:zoom/cto.md", false).unwrap();
        let flat = root.join("cto.md");
        write_managed_copy(&src, &flat, &root, "agent:zoom/cto.md", false).unwrap();

        prune_other_paths_for_item(&root, "agent:zoom/cto.md", &flat).unwrap();

        assert!(flat.is_file(), "flat copy kept");
        assert!(!nested.exists(), "nested orphan removed");
        assert!(!root.join("zoom").exists(), "now-empty dir removed");
        assert!(
            read_entry(&root, &nested).is_none(),
            "nested manifest entry dropped"
        );
        assert!(
            read_entry(&root, &flat).is_some(),
            "flat manifest entry kept"
        );
    }

    #[test]
    fn prune_keeps_copies_of_other_items() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        let src_a = dir.path().join("src/a.md");
        let src_b = dir.path().join("src/b.md");
        std::fs::create_dir_all(src_a.parent().unwrap()).unwrap();
        std::fs::write(&src_a, "a").unwrap();
        std::fs::write(&src_b, "b").unwrap();
        let a = root.join("a.md");
        let b = root.join("b.md");
        write_managed_copy(&src_a, &a, &root, "agent:a.md", false).unwrap();
        write_managed_copy(&src_b, &b, &root, "agent:b.md", false).unwrap();

        // Pruning item a (keeping its only copy) must not touch item b.
        prune_other_paths_for_item(&root, "agent:a.md", &a).unwrap();
        assert!(a.is_file() && b.is_file(), "other items untouched");
    }

    #[test]
    fn atomic_file_copy_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        let source = dir.path().join("src/agent.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "agent body").unwrap();
        let target = root.join("agent.md");

        write_managed_copy(&source, &target, &root, "agent:agent.md", true).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "agent body");
        assert!(
            !target.with_extension("agentic.tmp").exists(),
            "staging file cleaned up"
        );
    }

    #[test]
    fn read_manifest_tolerates_malformed_and_wrong_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("agent.md");

        // Malformed JSON -> empty manifest.
        std::fs::write(root.join(MANIFEST_FILE), "{ not json").unwrap();
        assert!(read_entry(&root, &target).is_none());

        // A future schema version is ignored (treated as empty).
        std::fs::write(
            root.join(MANIFEST_FILE),
            r#"{ "version": 2, "entries": { "agent.md": { "itemId": "x", "sourcePath": "/s", "sourceHash": "h" } } }"#,
        )
        .unwrap();
        assert!(read_entry(&root, &target).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn remove_existing_drops_symlink_not_its_target() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("real.md");
        std::fs::write(&source, "keep me").unwrap();
        let link = dir.path().join("link.md");
        symlink(&source, &link).unwrap();

        remove_existing(&link).unwrap();
        assert!(!link.exists(), "symlink removed");
        assert!(source.exists(), "link target untouched");
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "keep me");
    }

    #[test]
    fn content_hash_is_none_when_expected_content_absent() {
        let dir = tempfile::tempdir().unwrap();
        // A directory without SKILL.md has no content hash.
        let skill_dir = dir.path().join("skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        assert!(content_hash(&skill_dir).is_none());
        // A missing file likewise.
        assert!(content_hash(&dir.path().join("missing.md")).is_none());
    }

    #[test]
    fn rewrite_refreshes_content_and_source_hash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("agents");
        let source = dir.path().join("src/agent.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "v1").unwrap();
        let target = root.join("agent.md");

        write_managed_copy(&source, &target, &root, "agent:agent.md", false).unwrap();
        let first = read_entry(&root, &target).unwrap().source_hash;

        // Source drifts; rewriting refreshes the copy and the recorded hash.
        std::fs::write(&source, "v2 changed").unwrap();
        write_managed_copy(&source, &target, &root, "agent:agent.md", false).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "v2 changed");
        let second = read_entry(&root, &target).unwrap().source_hash;
        assert_ne!(first, second, "source hash updated on refresh");
        assert_eq!(second, content_hash(&source).unwrap());
    }
}
