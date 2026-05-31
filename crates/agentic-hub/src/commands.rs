//! Thin `#[tauri::command]` wrappers over `agentic_core`. No domain logic lives
//! here — only payload marshalling and error mapping. The read-only MVP surface:
//! settings load/save, scan, inspect, and source add/remove.

use std::collections::HashMap;

use agentic_core::api::{self, InspectResult};
use agentic_core::applier;
use agentic_core::managed_copy::now_iso8601;
use agentic_core::model::{
    ApplyError, ApplyResult, CapabilityItem, PlannedOperation, ScanResult, SyncRulesResult, ToolId,
};
use agentic_core::paths::expand_tilde;
use agentic_core::settings::{Settings, SourceConfig, ToolsSettings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::IpcError;

type IpcResult<T> = Result<T, IpcError>;

#[tauri::command]
pub async fn cmd_load_settings() -> IpcResult<Settings> {
    Ok(Settings::load()?)
}

#[tauri::command]
pub async fn cmd_save_settings(settings: Settings) -> IpcResult<()> {
    settings.save()?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ScanInput {
    pub sources: Vec<SourceConfig>,
}

#[tauri::command]
pub async fn cmd_scan(input: ScanInput) -> IpcResult<ScanResult> {
    // Reuse `resolve_sources` (slug assignment, tilde expansion, legacy
    // single-root fallback) by routing through a transient Settings.
    let settings = Settings {
        sources: input.sources,
        ..Settings::default()
    };
    Ok(api::scan(&settings))
}

#[tauri::command]
pub async fn cmd_inspect(
    items: Vec<CapabilityItem>,
    tools: ToolsSettings,
) -> IpcResult<InspectResult> {
    let settings = Settings {
        tools,
        ..Settings::default()
    };
    Ok(api::inspect(&items, &settings))
}

#[derive(Debug, Deserialize)]
pub struct AddSourceInput {
    pub label: String,
    pub path: String,
}

#[tauri::command]
pub async fn cmd_add_source(input: AddSourceInput) -> IpcResult<Settings> {
    let path = expand_tilde(&input.path);
    if !path.is_dir() {
        return Err(IpcError::new(
            "source_path_invalid",
            format!("Source path is not a directory: {}", path.display()),
        ));
    }
    let mut settings = Settings::load()?;
    settings.sources.push(SourceConfig {
        id: String::new(),
        label: input.label,
        path,
    });
    settings.save()?;
    Ok(settings)
}

#[derive(Debug, Deserialize)]
pub struct RemoveSourceInput {
    pub id: String,
}

#[tauri::command]
pub async fn cmd_remove_source(input: RemoveSourceInput) -> IpcResult<Settings> {
    let mut settings = Settings::load()?;
    // Persisted sources carry empty ids; resolved ids align positionally.
    let resolved = settings.resolve_sources();
    if let Some(pos) = resolved.iter().position(|s| s.id == input.id) {
        if pos < settings.sources.len() {
            settings.sources.remove(pos);
        }
    }
    settings.save()?;
    Ok(settings)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInput {
    pub tool_id: ToolId,
    pub items: Vec<CapabilityItem>,
    pub desired_enabled_by_item_id: HashMap<String, bool>,
}

#[tauri::command]
pub async fn cmd_plan(input: PlanInput) -> IpcResult<Vec<PlannedOperation>> {
    let settings = Settings::load()?;
    Ok(api::plan(
        &input.items,
        &settings,
        input.tool_id,
        &input.desired_enabled_by_item_id,
    ))
}

/// Per-operation progress event emitted during `cmd_apply`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProgressEvent {
    pub apply_id: String,
    pub operation_index: u32,
    pub total_operations: u32,
    pub operation: PlannedOperation,
    pub success: bool,
    pub error: Option<ApplyError>,
}

#[tauri::command]
pub async fn cmd_apply(
    app: AppHandle,
    operations: Vec<PlannedOperation>,
) -> IpcResult<ApplyResult> {
    let apply_id = now_iso8601();
    let total = operations.len() as u32;
    let result = applier::apply(&operations, |index, _, op, err| {
        let _ = app.emit(
            "apply-progress",
            ApplyProgressEvent {
                apply_id: apply_id.clone(),
                operation_index: index as u32,
                total_operations: total,
                operation: op.clone(),
                success: err.is_none(),
                error: err.cloned(),
            },
        );
    });
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRulesInput {
    pub tool_id: ToolId,
    pub items: Vec<CapabilityItem>,
    pub desired_enabled_by_item_id: HashMap<String, bool>,
}

#[tauri::command]
pub async fn cmd_sync_rules(input: SyncRulesInput) -> IpcResult<SyncRulesResult> {
    let settings = Settings::load()?;
    Ok(api::sync_rules(
        &input.items,
        &settings,
        input.tool_id,
        &input.desired_enabled_by_item_id,
    ))
}
