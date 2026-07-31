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
  queryUsageStats,
  scan,
  scanInstalledTools,
  scanWorkspace,
  resyncSuiteBinding,
  setWatcherEnabled,
  suiteOwnership,
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
  UsageStats,
} from "../types";
import {
  enabledTools,
  key,
  messageOf,
  WORKSPACE_ID_PREFIX,
  WORKSPACE_TOOL_IDS,
  WORKSPACE_TOOLS,
  type Scope,
  type ToolDef,
} from "../shared";

export type Status = "loading" | "ready" | "error";

export interface OwnershipInfo {
  suiteId: string;
  suiteName: string;
  fromBase: boolean;
}

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
  // Workspace scope renders the inventory read-only: no toggles, no apply.
  readOnly: boolean;
  // Global scope can include unmanaged tool-installed rows. Those individual
  // rows are audit-only even while normal source-root rows remain editable.
  readOnlyItemIds: Set<string>;
  // Settings-managed Agentic Hub rows are visible and must remain in sync
  // payloads, but their toggles are controlled from Config.
  settingsManagedItemIds: Set<string>;

  // Derived (recomputed on data/desired change).
  tools: ToolDef[];
  workspaceTools: ToolDef[];
  currentMap: Map<string, ToolCapabilityState>;
  pendingKeys: string[];
  // `key(tool, itemId)` -> owning suite, for cells a suite binding manages
  // (locked in the matrix). Empty when no suite is applied.
  ownership: Map<string, OwnershipInfo>;
  // Item id -> the skills.sh install behind it, so the row badge + "Update"
  // action can offer a one-click re-install. In workspace scope this is a
  // namespaced id keyed from the project's `skills-lock.json`; in global scope
  // it's keyed from a source root's `skills-lock.json` (a library install),
  // and carries `sourceId`/`destSubpath` so the update targets the right root.
  lockedSkills: Map<
    string,
    { name: string; source: string; sourceId?: string; destSubpath?: string }
  >;
  // Capability id -> local usage stats. Empty when tracing is disabled or the
  // usage DB is unavailable.
  usageStats: Map<string, UsageStats>;

  refresh: () => Promise<void>;
  loadWorkspace: (id: string) => Promise<void>;
  setScope: (scope: Scope) => void;
  toggleWatching: (next: boolean) => Promise<void>;
  /// Sync the watching flag from an external change (palette toggle) — the
  /// state is already persisted, so no IPC round-trip here.
  setWatching: (watching: boolean) => void;
  toggle: (tool: ToolId, itemId: string) => void;
  toggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  stageStaleRefresh: (tool: ToolId, itemId: string) => void;
  resyncStaleBinding: (tool: ToolId) => Promise<void>;
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

function namespaceItem(it: CapabilityItem): CapabilityItem {
  return { ...it, id: WORKSPACE_ID_PREFIX + it.id };
}

function namespaceState(s: ToolCapabilityState): ToolCapabilityState {
  return { ...s, itemId: WORKSPACE_ID_PREFIX + s.itemId };
}

function isSettingsManagedItem(it: CapabilityItem): boolean {
  return it.sourceId === "agentic-hub";
}

function filterInstalledInventory(
  installed: { items: CapabilityItem[]; states: ToolCapabilityState[]; errors: ScanError[] },
  managedStates: ToolCapabilityState[],
): { items: CapabilityItem[]; states: ToolCapabilityState[]; errors: ScanError[] } {
  const managedTargets = new Set(
    managedStates
      .filter((s) => s.state !== "disabled")
      .map((s) => s.targetPath),
  );
  const states = installed.states.filter((s) => !managedTargets.has(s.targetPath));
  const liveIds = new Set(states.map((s) => s.itemId));
  return {
    items: installed.items.filter((it) => liveIds.has(it.id)),
    states,
    errors: installed.errors,
  };
}

// Global resources project into each tool's home dir, so a globally-enabled
// resource applies to *every* project. Scan the shared roots, inspect, then
// keep only the states that are actually projected (enabled) for a workspace
// tool — those are the ones that genuinely "apply" to the audited project.
async function loadGlobalApplied(settings: Settings): Promise<{
  items: CapabilityItem[];
  states: ToolCapabilityState[];
  errors: ScanError[];
}> {
  const { items, errors } = await scan(settings.sources);
  const result = await inspect(items, settings.tools);
  const states = result.states.filter(
    (s) => s.state === "enabled" && WORKSPACE_TOOL_IDS.has(s.tool),
  );
  const liveIds = new Set(states.map((s) => s.itemId));
  return { items: items.filter((it) => liveIds.has(it.id)), states, errors };
}

// Suite-managed cells, keyed by `key(tool, itemId)`. A failure here must not
// break the matrix, so it degrades to an empty (no-lock) map.
async function loadOwnership(): Promise<Map<string, OwnershipInfo>> {
  const map = new Map<string, OwnershipInfo>();
  try {
    for (const o of await suiteOwnership()) {
      map.set(key(o.tool, o.itemId), {
        suiteId: o.suiteId,
        suiteName: o.suiteName,
        fromBase: o.fromBase,
      });
    }
  } catch {
    // Leave empty — cells stay editable.
  }
  return map;
}

async function loadUsageStats(items: CapabilityItem[]): Promise<Map<string, UsageStats>> {
  try {
    const rows = await queryUsageStats(items);
    return new Map(rows.map((row) => [row.capabilityId, row]));
  } catch {
    return new Map();
  }
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
  readOnly: false,
  readOnlyItemIds: new Set(),
  settingsManagedItemIds: new Set(),
  tools: [],
  workspaceTools: [],
  currentMap: new Map(),
  pendingKeys: [],
  ownership: new Map(),
  lockedSkills: new Map(),
  usageStats: new Map(),

  refresh: async () => {
    set({ status: "loading", error: "" });
    try {
      const settings = await loadSettings();
      const { items, errors, lockedSkills: locked } = await scan(settings.sources);
      const lockedSkills = new Map(
        locked.map((l) => [
          l.itemId,
          { name: l.name, source: l.source, sourceId: l.sourceId, destSubpath: l.destSubpath },
        ]),
      );
      const [managedResult, installedRaw] = await Promise.all([
        inspect(items, settings.tools),
        scanInstalledTools(settings.tools),
      ]);
      const installed = filterInstalledInventory(installedRaw, managedResult.states);
      const readOnlyItemIds = new Set(installed.items.map((it) => it.id));
      const result = {
        states: [...managedResult.states, ...installed.states],
        adapterStatuses: managedResult.adapterStatuses,
      };
      const allItems = [...items, ...installed.items];
      const settingsManagedItemIds = new Set(
        allItems.filter(isSettingsManagedItem).map((it) => it.id),
      );
      const desired = seedDesired(result);
      const currentMap = buildCurrentMap(result);
      const tools = enabledTools(settings);
      const [ownership, usageStats] = await Promise.all([
        loadOwnership(),
        loadUsageStats(allItems),
      ]);
      set({
        data: { settings, items: allItems, scanErrors: [...errors, ...installed.errors], result },
        desired,
        currentMap,
        tools,
        workspaceTools: tools.filter((t) => WORKSPACE_TOOL_IDS.has(t.id)),
        pendingKeys: [],
        ownership,
        lockedSkills,
        usageStats,
        readOnlyItemIds,
        settingsManagedItemIds,
        watching: settings.watcherEnabled,
        readOnly: false,
        status: "ready",
      });
    } catch (e) {
      set({ error: messageOf(e), status: "error" });
    }
  },

  // Workspace scope: a read-only audit of one project. It answers "what agentic
  // resources actually apply here?" by merging two sources:
  //   - Global: resources projected into each tool's home dir (apply to every
  //     project). Only the *enabled* ones for workspace tools are kept.
  //   - Local: the workspace's own tool dirs (`scanWorkspace`), always present.
  // Local ids are namespaced so a global and a local resource that share a path
  // stay distinct rows, each tagged with its own source for the source filter.
  // Only present resources are emitted, so `desired` mirrors `current` and
  // `pendingKeys` stays empty — there is nothing to apply.
  loadWorkspace: async (id) => {
    set({ status: "loading", error: "" });
    try {
      const settings = await loadSettings();
      const [global, inv] = await Promise.all([
        loadGlobalApplied(settings),
        scanWorkspace(id),
      ]);
      const items = [...global.items, ...inv.items.map(namespaceItem)];
      const states = [...global.states, ...inv.states.map(namespaceState)];
      const result: InspectResult = { states, adapterStatuses: [] };
      const currentMap = buildCurrentMap(result);
      // Key by the same namespaced id the matrix rows carry, so a row lookup is
      // a direct hit. The lock's `itemId` is the pre-namespace local id.
      const lockedSkills = new Map(
        inv.lockedSkills.map((l) => [
          WORKSPACE_ID_PREFIX + l.itemId,
          { name: l.name, source: l.source },
        ]),
      );
      const usageStats = await loadUsageStats(items);
      set({
        data: {
          settings,
          items,
          scanErrors: [...global.errors, ...inv.errors],
          result,
        },
        desired: seedDesired(result),
        currentMap,
        tools: WORKSPACE_TOOLS,
        workspaceTools: WORKSPACE_TOOLS,
        pendingKeys: [],
        ownership: new Map(),
        lockedSkills,
        usageStats,
        readOnlyItemIds: new Set(),
        settingsManagedItemIds: new Set(
          items.filter(isSettingsManagedItem).map((it) => it.id),
        ),
        readOnly: true,
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

  setWatching: (watching) => set({ watching }),

  toggle: (tool, itemId) => {
    const { currentMap, desired, ownership, readOnly, readOnlyItemIds, settingsManagedItemIds } =
      get();
    if (readOnly || readOnlyItemIds.has(itemId) || settingsManagedItemIds.has(itemId)) return;
    const k = key(tool, itemId);
    // Suite-managed cells are locked: a binding owns them.
    if (!currentMap.has(k) || ownership.has(k)) return;
    const nextDesired = { ...desired, [k]: !desired[k] };
    set({ desired: nextDesired, pendingKeys: computePending(nextDesired, currentMap) });
  },

  toggleMany: (tool, itemIds, value) => {
    const { currentMap, desired, ownership, readOnly, readOnlyItemIds, settingsManagedItemIds } =
      get();
    if (readOnly) return;
    const next = { ...desired };
    for (const id of itemIds) {
      if (readOnlyItemIds.has(id) || settingsManagedItemIds.has(id)) continue;
      const k = key(tool, id);
      if (currentMap.has(k) && !ownership.has(k)) next[k] = value;
    }
    set({ desired: next, pendingKeys: computePending(next, currentMap) });
  },

  stageStaleRefresh: (tool, itemId) => {
    const { currentMap, desired, ownership, readOnly, readOnlyItemIds } = get();
    if (readOnly || readOnlyItemIds.has(itemId)) return;
    const k = key(tool, itemId);
    if (ownership.has(k) || currentMap.get(k)?.state !== "stale") return;
    const nextDesired = { ...desired, [k]: true };
    set({ desired: nextDesired, pendingKeys: computePending(nextDesired, currentMap) });
  },

  resyncStaleBinding: async (tool) => {
    const staged = new Map(
      get().pendingKeys.map((pendingKey) => [pendingKey, get().desired[pendingKey]]),
    );
    try {
      const result = await resyncSuiteBinding(tool);
      await get().refresh();
      const refreshed = get();
      const desired = { ...refreshed.desired };
      for (const [pendingKey, value] of staged) {
        if (refreshed.currentMap.has(pendingKey)) desired[pendingKey] = value;
      }
      set({ desired, pendingKeys: computePending(desired, refreshed.currentMap) });

      const label = refreshed.tools.find((candidate) => candidate.id === tool)?.label ?? tool;
      const errorCount =
        result.applyResult.errors.length +
        result.ruleSync.errors.length +
        result.hookSync.errors.length;
      const changed =
        result.applyResult.created +
        result.applyResult.removed +
        result.applyResult.replaced +
        result.applyResult.refreshed;
      if (errorCount > 0 && changed === 0) {
        toast.error(
          `Suite binding re-sync failed for ${label} with ${errorCount} ${errorCount === 1 ? "error" : "errors"}.`,
        );
      } else if (errorCount > 0) {
        toast.warning(
          `Suite binding partially re-synced for ${label} with ${errorCount} ${errorCount === 1 ? "error" : "errors"}.`,
        );
      } else if (result.skippedStale + result.skippedAbsentSource > 0) {
        const skipped = result.skippedStale + result.skippedAbsentSource;
        toast.warning(
          `Suite binding re-synced for ${label} with ${skipped} skipped ${skipped === 1 ? "reference" : "references"}.`,
        );
      } else {
        toast.success(`Suite binding re-synced for ${label}.`);
      }
    } catch (error) {
      toast.error(messageOf(error));
    }
  },

  resetDesired: () => {
    const { data, currentMap } = get();
    if (!data) return;
    const desired = seedDesired(data.result);
    set({ desired, pendingKeys: computePending(desired, currentMap) });
  },

  setProgress: (p) => set({ progress: p }),

  requestApply: () => {
    const { data, pendingKeys, desired, currentMap, tools, readOnly } = get();
    if (!data || readOnly) return;
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
  const { data, desired, pendingKeys, tools, refresh, readOnlyItemIds } = get();
  if (!data) return;
  set({ applying: true, progress: null });
  try {
    const modifiedTools = new Set<ToolId>();
    for (const k of pendingKeys) modifiedTools.add(toolOfKey(k));
    const totals = { created: 0, removed: 0, replaced: 0, errors: 0 };
    for (const tool of tools) {
      if (!modifiedTools.has(tool.id)) continue;
      const editableItems = data.items.filter((item) => !readOnlyItemIds.has(item.id));
      const desiredByItem: DesiredMap = {};
      for (const item of editableItems) {
        const k = key(tool.id, item.id);
        if (k in desired) desiredByItem[item.id] = desired[k];
      }
      const ops = await plan(tool.id, editableItems, desiredByItem, takeOver);
      if (ops.length > 0) {
        const res: ApplyResult = await apply(ops);
        totals.created += res.created;
        totals.removed += res.removed;
        totals.replaced += res.replaced;
        totals.errors += res.errors.length;
      }
      await syncRules(tool.id, editableItems, desiredByItem);
      await syncHooks(tool.id, editableItems, desiredByItem);
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
