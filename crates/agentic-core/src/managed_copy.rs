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
