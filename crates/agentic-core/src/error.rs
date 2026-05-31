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

    #[error("failed to parse suite store: {0}")]
    SuiteParse(String),

    #[error("failed to parse workspace state: {0}")]
    StateParse(String),

    #[error("suite not found: {0}")]
    SuiteNotFound(String),

    #[error("a suite named \"{0}\" already exists")]
    SuiteNameConflict(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_stable() {
        assert_eq!(
            CoreError::SuiteNotFound("dev".into()).to_string(),
            "suite not found: dev"
        );
        assert_eq!(
            CoreError::SuiteNameConflict("dev".into()).to_string(),
            "a suite named \"dev\" already exists"
        );
        assert_eq!(
            CoreError::NotADirectory(PathBuf::from("/x")).to_string(),
            "path is not a directory: /x"
        );
    }

    #[test]
    fn io_errors_convert_via_from() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let err: CoreError = io.into();
        assert!(matches!(err, CoreError::Io(_)));
    }
}
