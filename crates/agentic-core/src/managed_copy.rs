//! Cursor managed-copy metadata sidecar.
//!
//! Cursor loads agent files into memory at launch, so symlinks are unreliable.
//! We write a real file plus a `<file>.e-studio-meta.json` sidecar that ties the
//! copy back to its shared source for stale detection. The `e-studio-` prefix is
//! preserved verbatim from the VS Code extension for migration parity.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Sidecar extension. `coding-agent.md` → `coding-agent.e-studio-meta.json`.
pub const META_EXTENSION: &str = "e-studio-meta.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCopyMeta {
    pub source_path: PathBuf,
    pub source_hash: String,
    pub synced_at: String,
}

/// Sidecar path for a managed-copy target.
pub fn meta_path(target: &Path) -> PathBuf {
    target.with_extension(META_EXTENSION)
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

/// Read and parse the sidecar for a target, if present and well-formed.
pub fn read_meta(target: &Path) -> Option<ManagedCopyMeta> {
    let content = std::fs::read_to_string(meta_path(target)).ok()?;
    serde_json::from_str(&content).ok()
}

/// Copy `source` to `target` and write its metadata sidecar. When `atomic`,
/// the content is staged to a `.tmp` sibling and renamed into place (used for
/// refresh-in-place of an existing managed copy).
pub fn write_managed_copy(source: &Path, target: &Path, atomic: bool) -> std::io::Result<()> {
    use std::fs;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = fs::read(source)?;
    if atomic {
        let tmp = target.with_extension("agentic.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, target)?;
    } else {
        fs::write(target, &bytes)?;
    }
    let meta = ManagedCopyMeta {
        source_path: source.to_path_buf(),
        source_hash: hash_bytes(&bytes),
        synced_at: now_iso8601(),
    };
    fs::write(meta_path(target), serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}

/// Remove a managed copy and its sidecar. Ignores a missing sidecar.
pub fn remove_managed_copy(target: &Path) -> std::io::Result<()> {
    std::fs::remove_file(target)?;
    let _ = std::fs::remove_file(meta_path(target));
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
