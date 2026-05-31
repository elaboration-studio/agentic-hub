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

/// Remove a managed copy (file or folder) and drop its manifest entry.
pub fn remove_managed_copy(target: &Path, target_root: &Path) -> std::io::Result<()> {
    remove_existing(target)?;
    let mut manifest = read_manifest(target_root);
    manifest.entries.remove(&entry_key(target_root, target));
    write_manifest(target_root, &manifest)
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
}
