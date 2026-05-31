use std::path::PathBuf;

use thiserror::Error;

/// Domain error for the core crate. The Tauri shell maps these into the typed
/// `IpcError` envelope (see `docs/tech/modules/tauri-ipc-contract.md`).
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("path is not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("failed to parse settings: {0}")]
    SettingsParse(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
