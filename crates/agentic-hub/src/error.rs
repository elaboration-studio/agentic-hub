//! The typed IPC error envelope. Every `#[tauri::command]` returns
//! `Result<T, IpcError>`, which surfaces in JS as a thrown error carrying a
//! stable `code` and human `message`. See `docs/tech/modules/tauri-ipc-contract.md`.

use agentic_core::CoreError;
use serde::Serialize;

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    /// Stable, machine-readable code (see the contract's error table).
    pub code: String,
    /// Human-readable message, suitable for surfacing in the UI.
    pub message: String,
    /// Structured detail for specific errors.
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub details: Option<serde_json::Value>,
}

impl IpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        IpcError {
            code: code.to_string(),
            message: message.into(),
            details: None,
        }
    }
}

impl From<CoreError> for IpcError {
    fn from(err: CoreError) -> Self {
        let code = match &err {
            CoreError::PathNotFound(_) => "path_not_found",
            CoreError::NotADirectory(_) => "source_path_invalid",
            CoreError::SettingsParse(_) => "manifest_malformed",
            CoreError::SuiteParse(_) => "suite_store_malformed",
            CoreError::StateParse(_) => "state_malformed",
            CoreError::SuiteNotFound(_) => "suite_not_found",
            CoreError::SuiteNameConflict(_) => "suite_name_conflict",
            CoreError::InvalidSkillRef(_) => "invalid_skill_ref",
            CoreError::UnknownProvider(_) => "unknown_provider",
            CoreError::SkillSearch(_) => "skill_search",
            CoreError::Io(_) | CoreError::Json(_) => "internal",
        };
        IpcError::new(code, err.to_string())
    }
}
