// Typed IPC wrappers — one flat module over the Rust `cmd_*` handlers.
// Payloads are single typed objects; argument keys match the Rust parameter
// names. See docs/tech/modules/tauri-ipc-contract.md.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApplyProgressEvent,
  ApplyResult,
  CapabilityItem,
  InspectResult,
  PlannedOperation,
  ScanResult,
  Settings,
  SourceConfig,
  SyncRulesResult,
  ToolId,
  ToolsSettings,
} from "./types";

export type DesiredMap = Record<string, boolean>;

export const loadSettings = (): Promise<Settings> => invoke("cmd_load_settings");

export const saveSettings = (settings: Settings): Promise<void> =>
  invoke("cmd_save_settings", { settings });

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
): Promise<PlannedOperation[]> =>
  invoke("cmd_plan", { input: { toolId, items, desiredEnabledByItemId } });

export const apply = (operations: PlannedOperation[]): Promise<ApplyResult> =>
  invoke("cmd_apply", { operations });

export const syncRules = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
): Promise<SyncRulesResult> =>
  invoke("cmd_sync_rules", { input: { toolId, items, desiredEnabledByItemId } });

export const onApplyProgress = (
  cb: (e: ApplyProgressEvent) => void,
): Promise<UnlistenFn> =>
  listen<ApplyProgressEvent>("apply-progress", (event) => cb(event.payload));
