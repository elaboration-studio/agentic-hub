// Core scan/inspect/stage/apply loop, extracted from the old App.tsx into a
// Zustand store. Holds loaded data, the staged "desired" map, and the
// plan -> apply -> sync pipeline. Derived values (tools, currentMap,
// pendingKeys) are recomputed inside mutating actions so components can read
// them without selector-identity churn.

import { create } from "zustand";
import { toast } from "sonner";
import {
  apply,
  inspect,
  loadSettings,
  plan,
  scan,
  setWatcherEnabled,
  syncHooks,
  syncRules,
  type DesiredMap,
} from "../ipc";
import type {
  ApplyResult,
  CapabilityItem,
  InspectResult,
  ScanError,
  Settings,
  ToolCapabilityState,
  ToolId,
} from "../types";
import {
  enabledTools,
  key,
  messageOf,
  WORKSPACE_TOOL_IDS,
  type Scope,
  type ToolDef,
} from "../shared";

export type Status = "loading" | "ready" | "error";

export interface ConflictRow {
  toolLabel: string;
  name: string;
  targetPath: string;
}

interface Loaded {
  settings: Settings;
  items: CapabilityItem[];
  scanErrors: ScanError[];
  result: InspectResult;
}

interface ManagerState {
  status: Status;
  error: string;
  data: Loaded | null;
  desired: DesiredMap;
  applying: boolean;
  progress: { done: number; total: number } | null;
  scope: Scope;
  watching: boolean;
  conflicts: ConflictRow[] | null;

  // Derived (recomputed on data/desired change).
  tools: ToolDef[];
  workspaceTools: ToolDef[];
  currentMap: Map<string, ToolCapabilityState>;
  pendingKeys: string[];

  refresh: () => Promise<void>;
  setScope: (scope: Scope) => void;
  toggleWatching: (next: boolean) => Promise<void>;
  toggle: (tool: ToolId, itemId: string) => void;
  toggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  resetDesired: () => void;
  setProgress: (p: { done: number; total: number } | null) => void;
  requestApply: () => void;
  resolveConflicts: (takeOver: boolean) => void;
  cancelConflicts: () => void;
}

function seedDesired(result: InspectResult): DesiredMap {
  const map: DesiredMap = {};
  for (const s of result.states) {
    map[key(s.tool, s.itemId)] = s.state === "enabled";
  }
  return map;
}

function buildCurrentMap(result: InspectResult): Map<string, ToolCapabilityState> {
  const map = new Map<string, ToolCapabilityState>();
  for (const s of result.states) map.set(key(s.tool, s.itemId), s);
  return map;
}

function computePending(
  desired: DesiredMap,
  currentMap: Map<string, ToolCapabilityState>,
): string[] {
  return Object.keys(desired).filter((k) => {
    const cur = currentMap.get(k);
    return cur ? desired[k] !== (cur.state === "enabled") : false;
  });
}

// `key()` is `${tool}::${itemId}`; itemId never contains "::".
function toolOfKey(k: string): ToolId {
  return k.slice(0, k.indexOf("::")) as ToolId;
}

function detectConflicts(
  pendingKeys: string[],
  desired: DesiredMap,
  currentMap: Map<string, ToolCapabilityState>,
  items: CapabilityItem[],
  tools: ToolDef[],
): ConflictRow[] {
  const rows: ConflictRow[] = [];
  for (const k of pendingKeys) {
    if (!desired[k]) continue;
    const cur = currentMap.get(k);
    if (!cur || cur.state !== "foreign_file") continue;
    const toolId = toolOfKey(k);
    const tool = tools.find((t) => t.id === toolId);
    const item = items.find((it) => it.id === cur.itemId);
    rows.push({
      toolLabel: tool?.label ?? toolId,
      name: item?.name ?? cur.itemId,
      targetPath: cur.targetPath,
    });
  }
  return rows;
}

export const useManagerStore = create<ManagerState>((set, get) => ({
  status: "loading",
  error: "",
  data: null,
  desired: {},
  applying: false,
  progress: null,
  scope: "global",
  watching: true,
  conflicts: null,
  tools: [],
  workspaceTools: [],
  currentMap: new Map(),
  pendingKeys: [],

  refresh: async () => {
    set({ status: "loading", error: "" });
    try {
      const settings = await loadSettings();
      const { items, errors } = await scan(settings.sources);
      const result = await inspect(items, settings.tools);
      const desired = seedDesired(result);
      const currentMap = buildCurrentMap(result);
      const tools = enabledTools(settings);
      set({
        data: { settings, items, scanErrors: errors, result },
        desired,
        currentMap,
        tools,
        workspaceTools: tools.filter((t) => WORKSPACE_TOOL_IDS.has(t.id)),
        pendingKeys: [],
        watching: settings.watcherEnabled,
        status: "ready",
      });
    } catch (e) {
      set({ error: messageOf(e), status: "error" });
    }
  },

  setScope: (scope) => set({ scope }),

  toggleWatching: async (next) => {
    set({ watching: next });
    try {
      await setWatcherEnabled(next);
    } catch (e) {
      set({ watching: !next });
      toast.error(messageOf(e));
    }
  },

  toggle: (tool, itemId) => {
    const { currentMap, desired } = get();
    const k = key(tool, itemId);
    if (!currentMap.has(k)) return;
    const nextDesired = { ...desired, [k]: !desired[k] };
    set({ desired: nextDesired, pendingKeys: computePending(nextDesired, currentMap) });
  },

  toggleMany: (tool, itemIds, value) => {
    const { currentMap, desired } = get();
    const next = { ...desired };
    for (const id of itemIds) {
      const k = key(tool, id);
      if (currentMap.has(k)) next[k] = value;
    }
    set({ desired: next, pendingKeys: computePending(next, currentMap) });
  },

  resetDesired: () => {
    const { data, currentMap } = get();
    if (!data) return;
    const desired = seedDesired(data.result);
    set({ desired, pendingKeys: computePending(desired, currentMap) });
  },

  setProgress: (p) => set({ progress: p }),

  requestApply: () => {
    const { data, pendingKeys, desired, currentMap, tools } = get();
    if (!data) return;
    const rows = detectConflicts(pendingKeys, desired, currentMap, data.items, tools);
    if (rows.length === 0) {
      void runApply(set, get, false);
      return;
    }
    set({ conflicts: rows });
  },

  resolveConflicts: (takeOver) => {
    set({ conflicts: null });
    void runApply(set, get, takeOver);
  },

  cancelConflicts: () => set({ conflicts: null }),
}));

// Run the plan -> apply -> sync pipeline for every tool with pending edits.
// `takeOver` authorizes the destructive resolution of `foreign_file` targets.
async function runApply(
  set: (partial: Partial<ManagerState>) => void,
  get: () => ManagerState,
  takeOver: boolean,
) {
  const { data, desired, pendingKeys, tools, refresh } = get();
  if (!data) return;
  set({ applying: true, progress: null });
  try {
    const modifiedTools = new Set<ToolId>();
    for (const k of pendingKeys) modifiedTools.add(toolOfKey(k));
    const totals = { created: 0, removed: 0, replaced: 0, errors: 0 };
    for (const tool of tools) {
      if (!modifiedTools.has(tool.id)) continue;
      const desiredByItem: DesiredMap = {};
      for (const item of data.items) {
        const k = key(tool.id, item.id);
        if (k in desired) desiredByItem[item.id] = desired[k];
      }
      const ops = await plan(tool.id, data.items, desiredByItem, takeOver);
      if (ops.length > 0) {
        const res: ApplyResult = await apply(ops);
        totals.created += res.created;
        totals.removed += res.removed;
        totals.replaced += res.replaced;
        totals.errors += res.errors.length;
      }
      await syncRules(tool.id, data.items, desiredByItem);
      await syncHooks(tool.id, data.items, desiredByItem);
    }
    await refresh();
    const summary = `${totals.created} added · ${totals.removed} removed · ${totals.replaced} replaced`;
    if (totals.errors > 0) toast.warning(`Applied with ${totals.errors} error(s) — ${summary}`);
    else toast.success(`Applied — ${summary}`);
  } catch (e) {
    toast.error(messageOf(e));
  } finally {
    set({ applying: false, progress: null });
  }
}
