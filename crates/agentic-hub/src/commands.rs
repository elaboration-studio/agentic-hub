//! Thin `#[tauri::command]` wrappers over `agentic_core`. No domain logic lives
//! here — only payload marshalling and error mapping. The read-only MVP surface:
//! settings load/save, scan, inspect, and source add/remove.

use std::collections::HashMap;

use agentic_core::adapter_registry::WORKSPACE_TOOL_IDS;
use agentic_core::api::{self, InspectResult};
use agentic_core::applier;
use agentic_core::cli_tools::{self, CliTool, CliToolStatus};
use agentic_core::managed_copy::now_iso8601;
use agentic_core::model::{
    ApplyError, ApplyResult, ApplySuiteResult, CapabilityItem, PlannedOperation, ScanResult,
    SuiteBinding, SuiteDefinition, SuiteOwnership, SyncHooksResult, SyncRulesResult, ToolId,
    UsageStats, WorkspaceTarget, WorkspaceTargetsState,
};
use agentic_core::open_targets;
use agentic_core::paths::expand_tilde;
use agentic_core::scaffold::{self, ScaffoldMode, ScaffoldResult};
use agentic_core::settings::{Settings, SourceConfig, ToolsSettings};
use agentic_core::skill_favorites::{SkillFavorite, SkillFavoritesState, SkillFavoritesStore};
use agentic_core::skill_source::{provider_for, SkillCliStatus, SkillSearchHit};
use agentic_core::suite_binding_store::SuiteBindingStore;
use agentic_core::suite_store::{SuiteCreateInput, SuiteStore, SuiteUpdateInput};
use agentic_core::workspace_inventory::{self, InstalledToolInventory, WorkspaceInventory};
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::IpcError;
use crate::palette;
use crate::paste::{self, PasteOutcome};
use crate::usage_collector::{self, UsageCollectorState, UsageTracerHooksSyncResult, UsageTracingStatus};
use crate::watcher::{self, WatcherState};

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
pub async fn cmd_save_settings(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    telemetry: State<'_, crate::telemetry::TelemetryState>,
    usage_collector: State<'_, UsageCollectorState>,
    mut settings: Settings,
) -> IpcResult<()> {
    if !agentic_core::settings::is_valid_shortcut(&settings.palette_shortcut) {
        return Err(IpcError::new(
            "invalid_shortcut",
            format!("Invalid palette shortcut: {}", settings.palette_shortcut),
        ));
    }
    // `cli_tools_path` points at a catalog whose entries are executed (see
    // `cli_tools::check_tool`). The WebView is untrusted, so it must never be
    // able to set an executable-defining path: preserve whatever is on disk
    // (hand-edited by the user) and discard any value the renderer sent.
    settings.cli_tools_path = Settings::load()?.cli_tools_path;
    settings.save()?;
    // Mirror the telemetry consent flag so a mid-session toggle takes effect at
    // once (gates the next tracked event without needing a restart).
    telemetry.set_enabled(settings.telemetry.enabled);
    if settings.usage_tracing.enabled && settings.usage_tracing.collector_token.is_empty() {
        settings.usage_tracing.collector_token = format!("trace-{}", uuid::Uuid::new_v4());
        settings.save()?;
    }
    usage_collector
        .apply_settings(&settings)
        .map_err(|e| IpcError::new("usage_collector_failed", e))?;
    usage_collector::sync_tracer_hooks(&settings)
        .map_err(|e| IpcError::new("usage_hooks_failed", e))?;
    // Source roots may have changed; re-subscribe if the watcher is running.
    watcher.restart_if_running(app.clone());
    // The summon accelerator may have changed; re-register it now.
    palette::register_palette_shortcut(&app, &settings.palette_shortcut)
        .map_err(|e| IpcError::new("shortcut_register_failed", e))?;
    Ok(())
}

/// Toggle the command-palette window (used by the View menu and any UI button).
#[tauri::command]
pub async fn cmd_toggle_palette(app: AppHandle) -> IpcResult<()> {
    palette::toggle_palette(&app);
    Ok(())
}

/// Show and focus the main window, then hide the palette. Used by palette
/// navigation commands that route back into the main window.
#[tauri::command]
pub async fn cmd_show_main(app: AppHandle) -> IpcResult<()> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    palette::hide_palette(&app);
    crate::telemetry::record_client_engagement(&app);
    Ok(())
}

/// Toggle the source watcher on/off, persisting the choice to settings.
#[tauri::command]
pub async fn cmd_set_watcher_enabled(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    enabled: bool,
) -> IpcResult<()> {
    let mut settings = Settings::load()?;
    settings.watcher_enabled = enabled;
    settings.save()?;
    watcher.set_enabled(app, enabled);
    Ok(())
}

/// Fallback recovery: full rescan + resync of every enabled tool (no newcomer
/// auto-enable) and the active workspace, then notify the UI. Use when the
/// watcher is off or projections look out of sync.
#[tauri::command]
pub async fn cmd_rescan_resync(app: AppHandle) -> IpcResult<()> {
    watcher::resync_now(&app);
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

#[derive(Debug, Deserialize)]
pub struct ScanInstalledToolsInput {
    pub tools: ToolsSettings,
}

#[tauri::command]
pub async fn cmd_scan_installed_tools(
    input: ScanInstalledToolsInput,
) -> IpcResult<InstalledToolInventory> {
    let settings = Settings {
        tools: input.tools,
        ..Settings::default()
    };
    Ok(workspace_inventory::scan_installed_tools(&settings))
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
#[serde(rename_all = "camelCase")]
pub struct ScaffoldDemoInput {
    pub mode: ScaffoldMode,
}

/// Materialize the bundled demo tree into the first configured source root
/// (the legacy `shared_root` when no sources are set), then nudge the watcher
/// so the new tree is picked up. First-run "empty start" affordance.
#[tauri::command]
pub async fn cmd_scaffold_demo(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    input: ScaffoldDemoInput,
) -> IpcResult<ScaffoldResult> {
    let settings = Settings::load()?;
    let dest = settings
        .resolve_sources()
        .into_iter()
        .next()
        .map(|s| s.path)
        .unwrap_or_else(|| settings.shared_root.clone());
    let result = scaffold::scaffold_demo(&dest, input.mode)?;
    watcher.restart_if_running(app);
    Ok(result)
}

/// Known workspace directories, used to extend the open-allowlist so a user can
/// open a workspace-projected file. Best-effort: a malformed state yields none.
fn workspace_dirs() -> Vec<PathBuf> {
    WorkspaceTargetStore::new()
        .read()
        .map(|s| s.workspace_targets.into_iter().map(|t| t.dir).collect())
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPathInput {
    pub path: String,
    /// App name/path to open with; `None` uses the OS default app.
    pub open_with: Option<String>,
}

/// Open a file with the user's preferred editor (or the OS default). The path
/// must canonicalize under a configured source root, tool target, or known
/// workspace dir — the WebView never holds opener/FS scope directly.
#[tauri::command]
pub async fn cmd_open_path(app: AppHandle, input: OpenPathInput) -> IpcResult<()> {
    let path = expand_tilde(&input.path);
    let settings = Settings::load()?;
    if !open_targets::is_openable(&path, &settings, &workspace_dirs()) {
        return Err(IpcError::new(
            "path_not_openable",
            format!(
                "Refusing to open a path outside known roots: {}",
                path.display()
            ),
        ));
    }
    app.opener()
        .open_path(path.to_string_lossy().to_string(), input.open_with)
        .map_err(|e| IpcError::new("open_failed", e.to_string()))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealPathInput {
    pub path: String,
}

/// Reveal a file in the system file explorer. Same allowlist as [`cmd_open_path`].
#[tauri::command]
pub async fn cmd_reveal_path(app: AppHandle, input: RevealPathInput) -> IpcResult<()> {
    let path = expand_tilde(&input.path);
    let settings = Settings::load()?;
    if !open_targets::is_openable(&path, &settings, &workspace_dirs()) {
        return Err(IpcError::new(
            "path_not_openable",
            format!(
                "Refusing to reveal a path outside known roots: {}",
                path.display()
            ),
        ));
    }
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| IpcError::new("reveal_failed", e.to_string()))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadBodyInput {
    pub path: String,
}

/// Read a capability file's text body for the palette's copy-to-clipboard
/// action (commands). Gated by the same allowlist as [`cmd_open_path`] so the
/// WebView can never read arbitrary files.
#[tauri::command]
pub async fn cmd_read_capability_body(input: ReadBodyInput) -> IpcResult<String> {
    let path = expand_tilde(&input.path);
    let settings = Settings::load()?;
    if !open_targets::is_openable(&path, &settings, &workspace_dirs()) {
        return Err(IpcError::new(
            "path_not_openable",
            format!(
                "Refusing to read a path outside known roots: {}",
                path.display()
            ),
        ));
    }
    std::fs::read_to_string(&path).map_err(|e| IpcError::new("read_failed", e.to_string()))
}

/// After the palette copies a command body, simulate Cmd+V into the frontmost
/// app (macOS, Accessibility-gated). Marshalling-only — logic lives in [`paste`].
#[tauri::command]
pub async fn cmd_paste_to_frontmost(app: AppHandle) -> IpcResult<PasteOutcome> {
    Ok(paste::paste_to_frontmost(&app))
}

#[derive(Debug, Deserialize)]
pub struct OpenUrlInput {
    pub url: String,
}

/// Open an external `http`/`https` URL in the user's default browser. Anchor
/// navigation is a no-op inside the WebView, so the UI routes external links
/// through here. The scheme is validated server-side — the WebView never holds
/// opener scope directly.
#[tauri::command]
pub async fn cmd_open_url(app: AppHandle, input: OpenUrlInput) -> IpcResult<()> {
    if !open_targets::is_safe_external_url(&input.url) {
        return Err(IpcError::new(
            "url_not_openable",
            format!("Refusing to open a non-http(s) URL: {}", input.url),
        ));
    }
    app.opener()
        .open_url(input.url.trim().to_string(), None::<&str>)
        .map_err(|e| IpcError::new("open_failed", e.to_string()))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct AddSourceInput {
    pub label: String,
    pub path: String,
}

#[tauri::command]
pub async fn cmd_add_source(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    input: AddSourceInput,
) -> IpcResult<Settings> {
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
    watcher.restart_if_running(app);
    Ok(settings)
}

#[derive(Debug, Deserialize)]
pub struct RemoveSourceInput {
    pub id: String,
}

#[tauri::command]
pub async fn cmd_remove_source(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    input: RemoveSourceInput,
) -> IpcResult<Settings> {
    let mut settings = Settings::load()?;
    // Persisted sources carry empty ids; resolved ids align positionally.
    let resolved = settings.resolve_sources();
    if let Some(pos) = resolved.iter().position(|s| s.id == input.id) {
        if pos < settings.sources.len() {
            settings.sources.remove(pos);
        }
    }
    settings.save()?;
    watcher.restart_if_running(app);
    Ok(settings)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInput {
    pub tool_id: ToolId,
    pub items: Vec<CapabilityItem>,
    pub desired_enabled_by_item_id: HashMap<String, bool>,
    /// Confirmed destructive take-over of `foreign_file` targets. Defaults to
    /// `false`; the UI sets it only after the user confirms the warning dialog.
    #[serde(default)]
    pub force: bool,
}

#[tauri::command]
pub async fn cmd_plan(input: PlanInput) -> IpcResult<Vec<PlannedOperation>> {
    let settings = Settings::load()?;
    Ok(api::plan(
        &input.items,
        &settings,
        input.tool_id,
        &input.desired_enabled_by_item_id,
        input.force,
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

#[tauri::command]
pub async fn cmd_usage_tracing_status(
    usage_collector: State<'_, UsageCollectorState>,
) -> IpcResult<UsageTracingStatus> {
    let settings = Settings::load()?;
    Ok(usage_collector.status(&settings))
}

#[tauri::command]
pub async fn cmd_set_usage_tracing_enabled(
    app: AppHandle,
    usage_collector: State<'_, UsageCollectorState>,
    enabled: bool,
) -> IpcResult<UsageTracingStatus> {
    let mut settings = Settings::load()?;
    settings.usage_tracing.enabled = enabled;
    if enabled && settings.usage_tracing.collector_token.is_empty() {
        settings.usage_tracing.collector_token = format!("trace-{}", uuid::Uuid::new_v4());
    }
    settings.save()?;
    usage_collector
        .apply_settings(&settings)
        .map_err(|e| IpcError::new("usage_collector_failed", e))?;
    usage_collector::sync_tracer_hooks(&settings)
        .map_err(|e| IpcError::new("usage_hooks_failed", e))?;
    let _ = app.emit("sources-changed", ());
    Ok(usage_collector.status(&settings))
}

#[tauri::command]
pub async fn cmd_sync_usage_tracer_hooks(
    usage_collector: State<'_, UsageCollectorState>,
) -> IpcResult<UsageTracerHooksSyncResult> {
    let mut settings = Settings::load()?;
    if settings.usage_tracing.enabled && settings.usage_tracing.collector_token.is_empty() {
        settings.usage_tracing.collector_token = format!("trace-{}", uuid::Uuid::new_v4());
        settings.save()?;
    }
    usage_collector
        .apply_settings(&settings)
        .map_err(|e| IpcError::new("usage_collector_failed", e))?;
    usage_collector::sync_tracer_hooks(&settings)
        .map_err(|e| IpcError::new("usage_hooks_failed", e))?;
    Ok(UsageTracerHooksSyncResult {
        status: usage_collector.status(&settings),
        synced_tools: usage_collector::synced_tracer_tools(&settings),
    })
}

#[tauri::command]
pub async fn cmd_query_usage_stats(items: Vec<CapabilityItem>) -> IpcResult<Vec<UsageStats>> {
    usage_collector::query_usage_stats(&items).map_err(|e| IpcError::new("usage_query_failed", e))
}

#[tauri::command]
pub async fn cmd_record_command_palette_usage(
    capability_id: String,
    pasted: bool,
) -> IpcResult<()> {
    let settings = Settings::load()?;
    if !settings.usage_tracing.enabled {
        return Ok(());
    }
    let scan = api::scan(&settings);
    usage_collector::record_command_palette_usage(&capability_id, pasted, &scan.items)
        .map_err(|e| IpcError::new("usage_record_failed", e))?;
    Ok(())
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

/// Resolve the effective suite for a bound suite id (selected ∪ base).
fn effective_for_suite_id(
    store: &SuiteStore,
    suite_id: &str,
    base: Option<&SuiteDefinition>,
    items: &[CapabilityItem],
) -> IpcResult<SuiteDefinition> {
    let mut suite = store
        .get(suite_id)?
        .ok_or_else(|| IpcError::new("suite_not_found", "Bound suite no longer exists"))?;
    SuiteStore::backfill_sources(&mut suite, items);
    Ok(api::merge_base_caps(&suite, base))
}

/// Re-apply each binding as a base-merged full reset, serialized against the
/// watcher so the two never write the same dirs. Each binding's selected suite
/// is resolved fresh and unioned with the current base.
fn resync_bindings(
    store: &SuiteStore,
    settings: &Settings,
    scanned: &ScanResult,
    bindings: &[SuiteBinding],
) {
    let base = store.base().ok().flatten();
    watcher::with_reconcile_guard(|| {
        for b in bindings {
            if let Ok(Some(selected)) = store.get(&b.suite_id) {
                let effective = api::merge_base_caps(&selected, base.as_ref());
                let enabled = api::enabled_item_ids(&scanned.items, settings, b.tool_id);
                let manual = api::manual_extras_from_enabled(&enabled, &effective, &scanned.items);
                let manual_refs: Vec<&str> = manual.iter().map(String::as_str).collect();
                let result = api::apply_suite(
                    &scanned.items,
                    settings,
                    b.tool_id,
                    &effective,
                    &manual_refs,
                );
                let _ =
                    SuiteBindingStore::new().record(b.tool_id, &b.suite_id, result.manual_item_ids);
            }
        }
    });
}

#[tauri::command]
pub async fn cmd_update_suite(
    app: AppHandle,
    input: UpdateSuiteInput,
) -> IpcResult<SuiteDefinition> {
    let store = suite_store()?;
    let suite = store.update(&input.id, input.changes)?;
    // Dynamic binding sync: a capability edit re-applies (full reset) to the
    // bound tools so their projections track the new set. When the edited suite
    // is the base, it merges into every applied suite, so re-sync ALL bindings;
    // otherwise only the tools bound to this suite.
    let binding_store = SuiteBindingStore::new();
    let to_resync: Vec<SuiteBinding> = if suite.is_base {
        binding_store.read()?
    } else {
        binding_store
            .read()?
            .into_iter()
            .filter(|b| b.suite_id == suite.id)
            .collect()
    };
    if !to_resync.is_empty() {
        let settings = Settings::load()?;
        let scanned = api::scan(&settings);
        resync_bindings(&store, &settings, &scanned, &to_resync);
        // Tool projections changed — nudge the manager to refresh.
        let _ = app.emit("sources-changed", ());
    }
    emit_suite_changed(&app, "updated", Some(suite.id.clone()));
    Ok(suite)
}

#[tauri::command]
pub async fn cmd_delete_suite(app: AppHandle, id: String) -> IpcResult<()> {
    suite_store()?.remove(&id)?;
    // Drop any tool bindings to the gone suite; on-disk projections are left
    // untouched (deleting a suite is not a destructive tool wipe).
    let _ = SuiteBindingStore::new().drop_suite(&id);
    emit_suite_changed(&app, "deleted", Some(id));
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySuiteInput {
    pub tool_id: ToolId,
    pub suite_id: String,
    #[serde(default)]
    pub preserve_manual: bool,
}

#[tauri::command]
pub async fn cmd_apply_suite(
    app: AppHandle,
    input: ApplySuiteInput,
) -> IpcResult<ApplySuiteResult> {
    let settings = Settings::load()?;
    let store = SuiteStore::with_path(settings.resolved_suites_path());
    let mut suite = store
        .get(&input.suite_id)?
        .ok_or_else(|| IpcError::new("suite_not_found", "Suite no longer exists"))?;

    let scanned = api::scan(&settings);
    // Qualify unqualified refs in-memory so apply matching is source-precise.
    // We deliberately do NOT persist this: rewriting the synced suites file
    // behind the user's back makes two devices diverge, and a later `git pull`
    // line-merges the divergent multi-line capability arrays into an empty set.
    SuiteStore::backfill_sources(&mut suite, &scanned.items);
    // Union the base suite's capabilities so its rules/skills are always present.
    let base = store.base()?;
    let effective = api::merge_base_caps(&suite, base.as_ref());
    let binding_store = SuiteBindingStore::new();
    let prior = binding_store.get(input.tool_id)?;
    let prior_effective = match prior.as_ref() {
        Some(b) => Some(effective_for_suite_id(
            &store,
            &b.suite_id,
            base.as_ref(),
            &scanned.items,
        )?),
        None => None,
    };
    let manual_to_preserve: Vec<String> = if input.preserve_manual {
        api::suite_apply_manual_extras(
            prior_effective.as_ref(),
            &effective,
            &scanned.items,
            &settings,
            input.tool_id,
        )
    } else {
        vec![]
    };
    let manual_refs: Vec<&str> = manual_to_preserve.iter().map(String::as_str).collect();
    let result = api::apply_suite(
        &scanned.items,
        &settings,
        input.tool_id,
        &effective,
        &manual_refs,
    );
    // Bind this tool to the selected suite (not the base) so a later capability
    // edit re-syncs it.
    let _ = binding_store.record(
        input.tool_id,
        &input.suite_id,
        result.manual_item_ids.clone(),
    );
    // Tool projections + suite ownership changed — nudge the manager (this window
    // or the main window when applied from the palette) to re-scan and re-lock.
    let _ = app.emit("sources-changed", ());
    Ok(result)
}

#[tauri::command]
pub async fn cmd_suite_apply_preview(input: ApplySuiteInput) -> IpcResult<Vec<String>> {
    let settings = Settings::load()?;
    let store = SuiteStore::with_path(settings.resolved_suites_path());
    let mut suite = store
        .get(&input.suite_id)?
        .ok_or_else(|| IpcError::new("suite_not_found", "Suite no longer exists"))?;
    let scanned = api::scan(&settings);
    SuiteStore::backfill_sources(&mut suite, &scanned.items);
    let base = store.base()?;
    let effective = api::merge_base_caps(&suite, base.as_ref());
    let prior = SuiteBindingStore::new().get(input.tool_id)?;
    let prior_effective = match prior.as_ref() {
        Some(b) => Some(effective_for_suite_id(
            &store,
            &b.suite_id,
            base.as_ref(),
            &scanned.items,
        )?),
        None => None,
    };
    Ok(api::suite_apply_manual_extras(
        prior_effective.as_ref(),
        &effective,
        &scanned.items,
        &settings,
        input.tool_id,
    ))
}

#[tauri::command]
pub async fn cmd_set_base_suite(app: AppHandle, id: Option<String>) -> IpcResult<()> {
    let store = suite_store()?;
    store.set_base(id.as_deref())?;
    // The base merges into every applied suite, so re-apply all bound tools so
    // their projections reflect the new (or cleared) base.
    let bindings = SuiteBindingStore::new().read()?;
    if !bindings.is_empty() {
        let settings = Settings::load()?;
        let scanned = api::scan(&settings);
        resync_bindings(&store, &settings, &scanned, &bindings);
    }
    // Always refresh the manager: the base set changed, so cell-lock ownership
    // must be recomputed even when no tool was re-applied (no bindings yet).
    let _ = app.emit("sources-changed", ());
    emit_suite_changed(&app, "base-changed", id);
    Ok(())
}

#[tauri::command]
pub async fn cmd_suite_ownership() -> IpcResult<Vec<SuiteOwnership>> {
    let settings = Settings::load()?;
    let store = SuiteStore::with_path(settings.resolved_suites_path());
    let scanned = api::scan(&settings);
    let suites = store.list()?;
    let base = store.base()?;
    let bindings = SuiteBindingStore::new().read()?;
    Ok(api::suite_ownership(
        &scanned.items,
        &bindings,
        &suites,
        base.as_ref(),
    ))
}

// ---- Workspace scope ------------------------------------------------------

#[tauri::command]
pub async fn cmd_pick_workspace_dir(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
) -> IpcResult<WorkspaceTarget> {
    let picked = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|fp| fp.as_path().map(|p| p.to_path_buf()))
        .ok_or_else(|| IpcError::new("dialog_cancelled", "No folder selected"))?;
    let target = WorkspaceTargetStore::new().add(&picked)?;
    watcher.restart_if_running(app);
    Ok(target)
}

#[tauri::command]
pub async fn cmd_list_workspace_targets() -> IpcResult<WorkspaceTargetsState> {
    Ok(WorkspaceTargetStore::new().read()?)
}

#[tauri::command]
pub async fn cmd_remove_workspace_target(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    id: String,
) -> IpcResult<()> {
    WorkspaceTargetStore::new().remove(&id)?;
    watcher.restart_if_running(app);
    Ok(())
}

#[tauri::command]
pub async fn cmd_set_active_workspace_target(
    app: AppHandle,
    watcher: State<'_, WatcherState>,
    id: String,
) -> IpcResult<()> {
    WorkspaceTargetStore::new().set_active(&id)?;
    watcher.restart_if_running(app);
    Ok(())
}

/// Read-only inventory of one workspace's installed agentic resources. Resolves
/// the target dir from the store, then walks each workspace tool's own dirs.
#[tauri::command]
pub async fn cmd_scan_workspace(workspace_id: String) -> IpcResult<WorkspaceInventory> {
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;
    Ok(workspace_inventory::scan_workspace(
        &target.dir,
        &WORKSPACE_TOOL_IDS,
    ))
}

// ---- CLI tool preflight ---------------------------------------------------

/// The effective tool catalog: the bundled set merged with the user's optional
/// local override (override by id, append new). A missing override file falls
/// back to the bundled set; a malformed one surfaces a typed error.
fn merged_cli_tools() -> IpcResult<Vec<CliTool>> {
    let bundled = cli_tools::bundled_catalog()?;
    let settings = Settings::load()?;
    let merged = match settings.resolved_cli_tools_path() {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(text) => cli_tools::merge_catalogs(bundled, cli_tools::parse_catalog(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bundled,
            Err(e) => return Err(IpcError::new("cli_tools_read_failed", e.to_string())),
        },
        None => bundled,
    };
    Ok(merged)
}

/// The CLI tool catalog the Tools panel renders (bundled + optional override).
#[tauri::command]
pub async fn cmd_list_tool_catalog() -> IpcResult<Vec<CliTool>> {
    merged_cli_tools()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckToolInput {
    pub id: String,
}

/// Probe one tool by id: run its check (installed + version) and, when
/// applicable, its auth command. Shells out, so it runs on the blocking pool.
#[tauri::command]
pub async fn cmd_check_tool(input: CheckToolInput) -> IpcResult<CliToolStatus> {
    let tool = merged_cli_tools()?
        .into_iter()
        .find(|t| t.id == input.id)
        .ok_or_else(|| IpcError::new("unknown_tool", "Unknown CLI tool id"))?;
    let status = tauri::async_runtime::spawn_blocking(move || cli_tools::check_tool(&tool))
        .await
        .map_err(|e| IpcError::new("internal", e.to_string()))?;
    Ok(status)
}

// ---- Skill sources (skills.sh) --------------------------------------------

/// Favorites store bound to the effective path from settings (custom override
/// or the canonical `~/.agentic-hub/skills-favorites.json`).
fn skill_favorites_store() -> IpcResult<SkillFavoritesStore> {
    Ok(SkillFavoritesStore::with_path(
        Settings::load()?.resolved_favorites_path(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct SkillProviderInput {
    pub provider: String,
}

/// Probe a provider's installer tooling (e.g. `npx`/Node for skills.sh) so the
/// Config panel can tell the user whether installs will work.
#[tauri::command]
pub async fn cmd_skill_cli_check(input: SkillProviderInput) -> IpcResult<SkillCliStatus> {
    let provider = provider_for(&input.provider)
        .ok_or_else(|| IpcError::new("unknown_provider", "Unknown skill source provider"))?;
    Ok(provider.cli_check())
}

#[derive(Debug, Deserialize)]
pub struct SearchSkillsInput {
    pub provider: String,
    pub query: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Search a provider's keyless public index. The WebView can't call skills.sh
/// directly (CORS), so discovery runs here in Rust — no API key involved. The
/// provider does a blocking HTTP GET, so it runs on the blocking pool rather
/// than a tokio worker.
#[tauri::command]
pub async fn cmd_search_skills(input: SearchSkillsInput) -> IpcResult<Vec<SkillSearchHit>> {
    if provider_for(&input.provider).is_none() {
        return Err(IpcError::new(
            "unknown_provider",
            "Unknown skill source provider",
        ));
    }
    let hits = tauri::async_runtime::spawn_blocking(move || {
        let provider = provider_for(&input.provider).expect("provider checked above");
        provider.search(&input.query, input.limit.unwrap_or(30))
    })
    .await
    .map_err(|e| IpcError::new("internal", e.to_string()))??;
    Ok(hits)
}

#[tauri::command]
pub async fn cmd_list_skill_favorites() -> IpcResult<SkillFavoritesState> {
    Ok(skill_favorites_store()?.read()?)
}

#[tauri::command]
pub async fn cmd_add_skill_favorite(favorite: SkillFavorite) -> IpcResult<SkillFavorite> {
    Ok(skill_favorites_store()?.add(favorite)?)
}

#[derive(Debug, Deserialize)]
pub struct RemoveFavoriteInput {
    pub provider: String,
    pub id: String,
}

#[tauri::command]
pub async fn cmd_remove_skill_favorite(input: RemoveFavoriteInput) -> IpcResult<()> {
    skill_favorites_store()?.remove(&input.provider, &input.id)?;
    Ok(())
}

// Skill install is a streaming, cancellable flow that owns its own window; see
// `install_window.rs` for `cmd_install_skill_stream` / `cmd_cancel_install`.
