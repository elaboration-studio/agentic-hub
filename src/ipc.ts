// Typed IPC wrappers — one flat module over the Rust `cmd_*` handlers.
// Payloads are single typed objects; argument keys match the Rust parameter
// names. See docs/tech/modules/tauri-ipc-contract.md.

import { invoke } from "@tauri-apps/api/core";
import type {
  CapabilityItem,
  InspectResult,
  ScanResult,
  Settings,
  SourceConfig,
  ToolsSettings,
} from "./types";

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
