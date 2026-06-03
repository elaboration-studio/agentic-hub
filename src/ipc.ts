// Typed IPC wrappers — one flat module over the Rust `cmd_*` handlers.
// Payloads are single typed objects; argument keys match the Rust parameter
// names. See docs/tech/modules/tauri-ipc-contract.md.

import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApplyProgressEvent,
  ApplyResult,
  ApplySuiteResult,
  CapabilityItem,
  InspectResult,
  PlannedOperation,
  ScaffoldMode,
  ScaffoldResult,
  ScanResult,
  Settings,
  SourceConfig,
  SuiteCapabilityRef,
  SuiteDefinition,
  SuiteStoreChangedEvent,
  SyncHooksResult,
  SyncRulesResult,
  ToolId,
  ToolsSettings,
  WorkspacePatchResult,
  WorkspaceTarget,
  WorkspaceTargetsState,
} from "./types";

export type DesiredMap = Record<string, boolean>;

export const loadSettings = (): Promise<Settings> => invoke("cmd_load_settings");

export const saveSettings = (settings: Settings): Promise<void> =>
  invoke("cmd_save_settings", { settings });

export const setWatcherEnabled = (enabled: boolean): Promise<void> =>
  invoke("cmd_set_watcher_enabled", { enabled });

// ---- Command palette + cross-window navigation ----------------------------

export type NavRoute = "manager" | "suites" | "config";

/// Show / hide the floating command-palette window (also bound to the View menu
/// and the global shortcut).
export const togglePalette = (): Promise<void> => invoke("cmd_toggle_palette");

/// Show + focus the main window and hide the palette. Used by palette nav.
export const showMain = (): Promise<void> => invoke("cmd_show_main");

/// Ask the main window to navigate to a route (emitted from the palette).
export const emitHubNavigate = (route: NavRoute): Promise<void> =>
  emit("hub-navigate", route);

/// Main window: react to a palette navigation request.
export const onHubNavigate = (cb: (route: NavRoute) => void): Promise<UnlistenFn> =>
  listen<NavRoute>("hub-navigate", (e) => cb(e.payload));

/// Main window: the "Settings…" menu item (Cmd+,) was activated.
export const onMenuOpenConfig = (cb: () => void): Promise<UnlistenFn> =>
  listen("menu-open-config", () => cb());

export const rescanResync = (): Promise<void> => invoke("cmd_rescan_resync");

/// Fired by the watcher (and the resync fallback) after projections change.
export const onSourcesChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("sources-changed", () => cb());

export const scan = (sources: SourceConfig[]): Promise<ScanResult> =>
  invoke("cmd_scan", { input: { sources } });

/// Materialize the bundled demo tree into the first source root. First-run
/// "empty start" affordance; re-scan after it resolves.
export const scaffoldDemo = (mode: ScaffoldMode): Promise<ScaffoldResult> =>
  invoke("cmd_scaffold_demo", { input: { mode } });

/// Open a file with the user's preferred editor (or system default when
/// `openWith` is absent). Validated server-side against known roots.
export const openPath = (path: string, openWith?: string): Promise<void> =>
  invoke("cmd_open_path", { input: { path, openWith: openWith ?? null } });

/// Reveal a file in the system file explorer (Finder / Explorer).
export const revealPath = (path: string): Promise<void> =>
  invoke("cmd_reveal_path", { input: { path } });

export const inspect = (
  items: CapabilityItem[],
  tools: ToolsSettings,
): Promise<InspectResult> => invoke("cmd_inspect", { items, tools });

export const addSource = (label: string, path: string): Promise<Settings> =>
  invoke("cmd_add_source", { input: { label, path } });

export const removeSource = (id: string): Promise<Settings> =>
  invoke("cmd_remove_source", { input: { id } });

export const plan = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
  force = false,
): Promise<PlannedOperation[]> =>
  invoke("cmd_plan", { input: { toolId, items, desiredEnabledByItemId, force } });

export const apply = (operations: PlannedOperation[]): Promise<ApplyResult> =>
  invoke("cmd_apply", { operations });

export const syncRules = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
): Promise<SyncRulesResult> =>
  invoke("cmd_sync_rules", { input: { toolId, items, desiredEnabledByItemId } });

export const syncHooks = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
): Promise<SyncHooksResult> =>
  invoke("cmd_sync_hooks", { input: { toolId, items, desiredEnabledByItemId } });

export const onApplyProgress = (
  cb: (e: ApplyProgressEvent) => void,
): Promise<UnlistenFn> =>
  listen<ApplyProgressEvent>("apply-progress", (event) => cb(event.payload));

// ---- Suites ---------------------------------------------------------------

export interface SuiteCreatePayload {
  name: string;
  description?: string | null;
  // Source-qualified refs. The Rust core also tolerates bare-string entries
  // (legacy), upgrading them in place on the next write.
  capabilities: SuiteCapabilityRef[];
}

export interface SuiteUpdatePayload {
  name?: string;
  description?: string | null;
  capabilities?: SuiteCapabilityRef[];
}

export const listSuites = (): Promise<SuiteDefinition[]> => invoke("cmd_list_suites");

export const getSuite = (id: string): Promise<SuiteDefinition | null> =>
  invoke("cmd_get_suite", { id });

export const createSuite = (input: SuiteCreatePayload): Promise<SuiteDefinition> =>
  invoke("cmd_create_suite", { input });

export const updateSuite = (
  id: string,
  changes: SuiteUpdatePayload,
): Promise<SuiteDefinition> => invoke("cmd_update_suite", { input: { id, ...changes } });

export const deleteSuite = (id: string): Promise<void> =>
  invoke("cmd_delete_suite", { id });

export const applySuite = (
  toolId: ToolId,
  suiteId: string,
): Promise<ApplySuiteResult> =>
  invoke("cmd_apply_suite", { input: { toolId, suiteId } });

export const onSuiteStoreChanged = (
  cb: (e: SuiteStoreChangedEvent) => void,
): Promise<UnlistenFn> =>
  listen<SuiteStoreChangedEvent>("suite-store-changed", (event) => cb(event.payload));

// ---- Workspace scope ------------------------------------------------------

export const pickWorkspaceDir = (): Promise<WorkspaceTarget> =>
  invoke("cmd_pick_workspace_dir");

export const listWorkspaceTargets = (): Promise<WorkspaceTargetsState> =>
  invoke("cmd_list_workspace_targets");

export const removeWorkspaceTarget = (id: string): Promise<void> =>
  invoke("cmd_remove_workspace_target", { id });

export const setActiveWorkspaceTarget = (id: string): Promise<void> =>
  invoke("cmd_set_active_workspace_target", { id });

export const applyWorkspacePatch = (
  workspaceId: string,
  toolId: ToolId,
  suiteId: string,
): Promise<WorkspacePatchResult> =>
  invoke("cmd_apply_workspace_patch", { input: { workspaceId, toolId, suiteId } });
