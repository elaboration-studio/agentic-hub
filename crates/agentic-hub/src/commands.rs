//! Thin `#[tauri::command]` wrappers over `agentic_core`. No domain logic lives
//! here — only payload marshalling and error mapping. The read-only MVP surface:
//! settings load/save, scan, inspect, and source add/remove.

use std::collections::HashMap;

use agentic_core::adapter_registry::WORKSPACE_TOOL_IDS;
use agentic_core::api::{self, InspectResult};
use agentic_core::applier;
use agentic_core::hook_sync;
use agentic_core::managed_copy::now_iso8601;
use agentic_core::model::{
    ApplyError, ApplyResult, ApplySuiteResult, CapabilityItem, PlannedOperation, ScanResult,
    SuiteDefinition, SyncHooksResult, SyncRulesResult, ToolId, WorkspacePatchResult,
    WorkspaceTarget, WorkspaceTargetsState,
};
use agentic_core::paths::expand_tilde;
use agentic_core::settings::{Settings, SourceConfig, ToolsSettings};
use agentic_core::suite_store::{SuiteCreateInput, SuiteStore, SuiteUpdateInput};
use agentic_core::workspace_patch;
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::error::IpcError;

type IpcResult<T> = Result<T, IpcError>;

/// Suite store bound to the effective suite-file path from settings (custom
/// override or the canonical `~/.agentic-suites.json`).
fn suite_store() -> IpcResult<SuiteStore> {
    Ok(SuiteStore::with_path(
        Settings::load()?.resolved_suites_path(),
    ))
}

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHooksInput {
    pub tool_id: ToolId,
    pub items: Vec<CapabilityItem>,
    pub desired_enabled_by_item_id: HashMap<String, bool>,
}

#[tauri::command]
pub async fn cmd_sync_hooks(input: SyncHooksInput) -> IpcResult<SyncHooksResult> {
    let settings = Settings::load()?;
    Ok(api::sync_hooks(
        &input.items,
        &settings,
        input.tool_id,
        &input.desired_enabled_by_item_id,
    ))
}

// ---- Suites ---------------------------------------------------------------

/// Notifies all windows when the suite store mutates, so the main window can
/// refresh its selector.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteStoreChangedEvent {
    pub kind: String,
    pub suite_id: Option<String>,
}

fn emit_suite_changed(app: &AppHandle, kind: &str, suite_id: Option<String>) {
    let _ = app.emit(
        "suite-store-changed",
        SuiteStoreChangedEvent {
            kind: kind.to_string(),
            suite_id,
        },
    );
}

#[tauri::command]
pub async fn cmd_list_suites() -> IpcResult<Vec<SuiteDefinition>> {
    Ok(suite_store()?.list()?)
}

#[tauri::command]
pub async fn cmd_get_suite(id: String) -> IpcResult<Option<SuiteDefinition>> {
    Ok(suite_store()?.get(&id)?)
}

#[tauri::command]
pub async fn cmd_create_suite(
    app: AppHandle,
    input: SuiteCreateInput,
) -> IpcResult<SuiteDefinition> {
    let suite = suite_store()?.create(input)?;
    emit_suite_changed(&app, "created", Some(suite.id.clone()));
    Ok(suite)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSuiteInput {
    pub id: String,
    #[serde(flatten)]
    pub changes: SuiteUpdateInput,
}

#[tauri::command]
pub async fn cmd_update_suite(
    app: AppHandle,
    input: UpdateSuiteInput,
) -> IpcResult<SuiteDefinition> {
    let suite = suite_store()?.update(&input.id, input.changes)?;
    emit_suite_changed(&app, "updated", Some(suite.id.clone()));
    Ok(suite)
}

#[tauri::command]
pub async fn cmd_delete_suite(app: AppHandle, id: String) -> IpcResult<()> {
    suite_store()?.remove(&id)?;
    emit_suite_changed(&app, "deleted", Some(id));
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySuiteInput {
    pub tool_id: ToolId,
    pub suite_id: String,
}

#[tauri::command]
pub async fn cmd_apply_suite(input: ApplySuiteInput) -> IpcResult<ApplySuiteResult> {
    let settings = Settings::load()?;
    let suite = SuiteStore::with_path(settings.resolved_suites_path())
        .get(&input.suite_id)?
        .ok_or_else(|| IpcError::new("suite_not_found", "Suite no longer exists"))?;

    let scanned = api::scan(&settings);
    Ok(api::apply_suite(
        &scanned.items,
        &settings,
        input.tool_id,
        &suite,
    ))
}

// ---- Workspace scope ------------------------------------------------------

#[tauri::command]
pub async fn cmd_pick_workspace_dir(app: AppHandle) -> IpcResult<WorkspaceTarget> {
    let picked = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|fp| fp.as_path().map(|p| p.to_path_buf()))
        .ok_or_else(|| IpcError::new("dialog_cancelled", "No folder selected"))?;
    Ok(WorkspaceTargetStore::new().add(&picked)?)
}

#[tauri::command]
pub async fn cmd_list_workspace_targets() -> IpcResult<WorkspaceTargetsState> {
    Ok(WorkspaceTargetStore::new().read()?)
}

#[tauri::command]
pub async fn cmd_remove_workspace_target(id: String) -> IpcResult<()> {
    WorkspaceTargetStore::new().remove(&id)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_set_active_workspace_target(id: String) -> IpcResult<()> {
    WorkspaceTargetStore::new().set_active(&id)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWorkspacePatchInput {
    pub workspace_id: String,
    pub tool_id: ToolId,
    pub suite_id: String,
}

#[tauri::command]
pub async fn cmd_apply_workspace_patch(
    input: ApplyWorkspacePatchInput,
) -> IpcResult<WorkspacePatchResult> {
    if !WORKSPACE_TOOL_IDS.contains(&input.tool_id) {
        return Err(IpcError::new(
            "tool_unsupported_in_workspace",
            "This tool is not supported in workspace scope",
        ));
    }
    let store = WorkspaceTargetStore::new();
    let target = store
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == input.workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;

    let settings = Settings::load()?;
    let suite = SuiteStore::with_path(settings.resolved_suites_path())
        .get(&input.suite_id)?
        .ok_or_else(|| IpcError::new("suite_not_found", "Suite no longer exists"))?;

    let scanned = api::scan(&settings);
    let manifests = hook_sync::load_manifests(&scanned.items);

    let result = workspace_patch::apply_workspace_patch(
        &target.dir,
        input.tool_id,
        &suite,
        &scanned.items,
        &manifests,
    )?;
    let _ = store.set_active(&input.workspace_id);
    Ok(result)
}
