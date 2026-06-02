use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::model::{CapabilityItem, CapabilityKind, ScanError, ScanResult};
use crate::settings::SourceConfig;

/// Bounded walk depth; guards against symlink cycles inside a source root.
const MAX_DEPTH: usize = 16;

/// Reserved folder name holding old versions of files. Never scanned.
const ARCHIVED: &str = "__archived__";

/// Scan a single shared root (legacy single-source entry). Thin wrapper over
/// [`scan_all`].
pub fn scan(shared_root: &Path) -> ScanResult {
    let source = SourceConfig {
        id: "default".to_string(),
        label: "Default".to_string(),
        path: shared_root.to_path_buf(),
    };
    scan_all(&[source])
}

/// Scan every source in priority order. Items are keyed by
/// `${kind}:${relative_path}`; the first source to provide a key wins and any
/// later duplicate is dropped and reported as a shadowing [`ScanError`].
pub fn scan_all(sources: &[SourceConfig]) -> ScanResult {
    let mut errors: Vec<ScanError> = Vec::new();
    // key -> (item, winning source label)
    let mut by_key: HashMap<String, (CapabilityItem, String)> = HashMap::new();
    // preserve first-seen order for stable output before the final sort
    let mut order: Vec<String> = Vec::new();

    for source in sources {
        if !source.path.is_dir() {
            errors.push(ScanError {
                path: source.path.clone(),
                message: "source folder is missing or not a directory".to_string(),
            });
            continue;
        }

        let mut found: Vec<CapabilityItem> = Vec::new();
        for kind in CapabilityKind::ALL {
            let base = source.path.join(kind.dir_name());
            if base.is_dir() {
                walk(&base, &base, kind, source, 0, &mut found, &mut errors);
            }
        }

        for item in found {
            let key = format!(
                "{}:{}",
                item.kind.id_prefix(),
                rel_to_unix(&item.relative_path)
            );
            if let Some((_, winner_label)) = by_key.get(&key) {
                errors.push(ScanError {
                    path: item.source_path.clone(),
                    message: format!(
                        "shadowed by higher-priority source \"{winner_label}\", which already provides {key}"
                    ),
                });
            } else {
                order.push(key.clone());
                by_key.insert(key, (item, source.label.clone()));
            }
        }
    }

    let mut items: Vec<CapabilityItem> = order
        .into_iter()
        .filter_map(|k| by_key.remove(&k).map(|(item, _)| item))
        .collect();
    // Deterministic output regardless of OS directory-read order.
    items.sort_by(|a, b| a.id.cmp(&b.id));

    ScanResult { items, errors }
}

fn walk(
    base: &Path,
    dir: &Path,
    kind: CapabilityKind,
    source: &SourceConfig,
    depth: usize,
    out: &mut Vec<CapabilityItem>,
    errors: &mut Vec<ScanError>,
) {
    if depth > MAX_DEPTH {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(ScanError {
                path: dir.to_path_buf(),
                message: e.to_string(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // `is_dir`/`is_file` follow symlinks, matching the contract that
        // symlinks inside a source root are resolved for validation.
        if path.is_dir() {
            if name == ARCHIVED {
                continue;
            }
            if let Some(marker) = kind.marker_file() {
                if path.join(marker).is_file() {
                    let mut item = dir_item(base, &path, kind, source);
                    // Hook identity follows the manifest `id` (folder name is the
                    // fallback), matching the VS Code extension's scanner.
                    if kind == CapabilityKind::Hook {
                        if let Ok(m) = crate::hook_sync::load_manifest(&path) {
                            item.id = format!("{}:{}", kind.id_prefix(), m.id);
                            item.name = m.name.unwrap_or(m.id);
                        }
                    }
                    out.push(item);
                }
            }
            walk(base, &path, kind, source, depth + 1, out, errors);
        } else if path.is_file() && has_allowed_ext(&path, kind.file_extensions()) {
            out.push(file_item(base, &path, kind, source));
        }
    }
}

/// Directory-marker item (skill / hook). `source_path` is the folder.
fn dir_item(
    base: &Path,
    path: &Path,
    kind: CapabilityKind,
    source: &SourceConfig,
) -> CapabilityItem {
    let rel = path.strip_prefix(base).unwrap_or(path).to_path_buf();
    let name = rel
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    CapabilityItem {
        id: format!("{}:{}", kind.id_prefix(), rel_to_unix(&rel)),
        kind,
        name,
        source_path: path.to_path_buf(),
        relative_path: rel,
        source_id: source.id.clone(),
        source_label: source.label.clone(),
        valid: true,
        validation_errors: Vec::new(),
    }
}

/// File-based item (agent / rule). `relative_path` and `id` include the
/// extension; the display name is the file stem.
fn file_item(
    base: &Path,
    path: &Path,
    kind: CapabilityKind,
    source: &SourceConfig,
) -> CapabilityItem {
    let rel = path.strip_prefix(base).unwrap_or(path).to_path_buf();
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    CapabilityItem {
        id: format!("{}:{}", kind.id_prefix(), rel_to_unix(&rel)),
        kind,
        name,
        source_path: path.to_path_buf(),
        relative_path: rel,
        source_id: source.id.clone(),
        source_label: source.label.clone(),
        valid: true,
        validation_errors: Vec::new(),
    }
}

fn has_allowed_ext(path: &Path, allowed: &[&str]) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            allowed.iter().any(|a| *a == ext)
        }
        None => false,
    }
}

fn rel_to_unix(rel: &Path) -> String {
    rel.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn source(id: &str, label: &str, path: &Path) -> SourceConfig {
        SourceConfig {
            id: id.to_string(),
            label: label.to_string(),
            path: path.to_path_buf(),
        }
    }

    #[test]
    fn scans_all_four_kinds_nested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("skills/dev/repo-research/SKILL.md"), "# skill");
        write(&root.join("agents/coding/coding-agent.md"), "# agent");
        write(&root.join("rules/general/precise.mdc"), "> rule");
        write(&root.join("hooks/auto-format-after-edit/hook.json"), "{}");

        let result = scan(root);
        assert!(result.errors.is_empty(), "{:?}", result.errors);

        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"skill:dev/repo-research"));
        assert!(ids.contains(&"agent:coding/coding-agent.md"));
        assert!(ids.contains(&"rule:general/precise.mdc"));
        assert!(ids.contains(&"hook:auto-format-after-edit"));

        let skill = result
            .items
            .iter()
            .find(|i| i.id == "skill:dev/repo-research")
            .unwrap();
        assert_eq!(skill.kind, CapabilityKind::Skill);
        assert_eq!(skill.name, "repo-research");
        assert!(skill.source_path.ends_with("skills/dev/repo-research"));
    }

    #[test]
    fn skips_archived_at_any_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("skills/live/SKILL.md"), "# live");
        write(&root.join("skills/__archived__/old/SKILL.md"), "# old");
        write(
            &root.join("skills/dev/__archived__/v1/SKILL.md"),
            "# old nested",
        );

        let result = scan(root);
        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["skill:live"]);
    }

    #[test]
    fn ignores_disallowed_extensions_and_markerless_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("rules/notes.txt"), "nope");
        write(&root.join("rules/keep.md"), "yes");
        // a skills subdir without SKILL.md is not a skill
        fs::create_dir_all(root.join("skills/empty-dir")).unwrap();

        let result = scan(root);
        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["rule:keep.md"]);
    }

    #[test]
    fn missing_source_reports_error_not_panic() {
        let result = scan(Path::new("/nonexistent/path/xyz"));
        assert!(result.items.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0]
            .message
            .contains("missing or not a directory"));
    }

    #[test]
    fn cross_source_shadowing_first_wins() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        write(&dir_a.path().join("skills/dev/tdd/SKILL.md"), "# A");
        write(&dir_b.path().join("skills/dev/tdd/SKILL.md"), "# B");
        write(&dir_b.path().join("skills/extra/SKILL.md"), "# only in B");

        let result = scan_all(&[
            source("arno", "Arno", dir_a.path()),
            source("team", "Team", dir_b.path()),
        ]);

        // tdd resolves to A (first wins); extra comes from B.
        let tdd = result
            .items
            .iter()
            .find(|i| i.id == "skill:dev/tdd")
            .unwrap();
        assert_eq!(tdd.source_label, "Arno");
        assert!(result.items.iter().any(|i| i.id == "skill:extra"));

        // B's tdd is shadowed and reported.
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0]
            .message
            .contains("shadowed by higher-priority source \"Arno\""));
        assert!(result.errors[0].message.contains("skill:dev/tdd"));
    }

    #[test]
    fn carries_source_identity() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("agents/foo.md"), "# foo");
        let result = scan_all(&[source("arno", "Arno", dir.path())]);
        let item = &result.items[0];
        assert_eq!(item.source_id, "arno");
        assert_eq!(item.source_label, "Arno");
    }

    #[test]
    fn hook_manifest_overrides_folder_id_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Folder is "folder-name" but the manifest carries its own identity.
        write(
            &root.join("hooks/folder-name/hook.json"),
            r#"{ "id": "custom-id", "name": "Custom Name", "command": "run", "events": [{"name":"Stop"}] }"#,
        );

        let result = scan(root);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let hook = result
            .items
            .iter()
            .find(|i| i.kind == CapabilityKind::Hook)
            .unwrap();
        assert_eq!(hook.id, "hook:custom-id");
        assert_eq!(hook.name, "Custom Name");
    }

    #[cfg(unix)]
    #[test]
    fn resolves_symlinked_skill_folder() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A real skill living outside the skills/ tree.
        let external = root.join("external/repo-research");
        write(&external.join("SKILL.md"), "# external");
        // Symlinked into the scanned source root.
        fs::create_dir_all(root.join("skills")).unwrap();
        symlink(&external, root.join("skills/linked")).unwrap();

        let result = scan(root);
        let ids: Vec<&str> = result.items.iter().map(|i| i.id.as_str()).collect();
        assert!(
            ids.contains(&"skill:linked"),
            "symlinked folder scanned: {ids:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_cycle_terminates_within_depth_bound() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let skill = root.join("skills/x");
        write(&skill.join("SKILL.md"), "# x");
        // A self-referential symlink would recurse forever without the guard.
        symlink(&skill, skill.join("loop")).unwrap();

        // The bounded walk returns rather than hanging or panicking.
        let result = scan(root);
        assert!(result.items.iter().any(|i| i.id == "skill:x"));
    }
}
