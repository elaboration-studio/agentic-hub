// Typed IPC wrappers — one flat module over the Rust `cmd_*` handlers.
// Payloads are single typed objects; argument keys match the Rust parameter
// names. See docs/tech/modules/tauri-ipc-contract.md.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApplyProgressEvent,
  ApplyResult,
  ApplySuiteResult,
  CapabilityItem,
  InspectResult,
  PlannedOperation,
  ScanResult,
  Settings,
  SourceConfig,
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

export const rescanResync = (): Promise<void> => invoke("cmd_rescan_resync");

/// Fired by the watcher (and the resync fallback) after projections change.
export const onSourcesChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("sources-changed", () => cb());

export const scan = (sources: SourceConfig[]): Promise<ScanResult> =>
  invoke("cmd_scan", { input: { sources } });

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
  capabilities: string[];
}

export interface SuiteUpdatePayload {
  name?: string;
  description?: string | null;
  capabilities?: string[];
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
