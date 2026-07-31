// Command-palette state: load resources + suites + settings once per summon,
// hold the query and selection, and derive the visible result list. The
// palette is layered: a `root` hub of first-class commands, a `search` view
// per drilled-in mode (kind search or locate scope), a `suite-tools` view
// reached through the suite mode (pick a tool to apply it to), and a
// `capability-tools` view reached from a resource row (toggle the resource on/
// off per tool, inline). Logic lives in the store (not the component) so the
// matching, navigation, selection, and toggling are unit-testable without a DOM.

import { create } from "zustand";
import {
  apply,
  emitHubLocate,
  emitHubNavigate,
  emitSourcesChanged,
  inspect,
  listSuites,
  listWorkspaceTargets,
  loadSettings,
  plan,
  scan,
  scanWorkspace,
  showMain,
  suiteOwnership,
  syncHooks,
  syncRules,
  type DesiredMap,
  type LocateRequest,
  type NavRoute,
} from "../ipc";
import type {
  CapabilityItem,
  PaletteLaunchMode,
  Settings,
  SuiteDefinition,
  ToolCapabilityState,
  ToolId,
} from "../types";
import { enabledTools, key, messageOf } from "../shared";
import {
  computeCapabilityToolResults,
  computeHubResults,
  computeSearchResults,
  computeSuiteToolResults,
  type CapabilityOwnership,
  type PaletteItem,
  type SearchMode,
  type WorkspaceInventoryEntry,
} from "../components/palette/commands";

export type Status = "loading" | "ready" | "error";

/// Which level of the palette is showing.
export type PaletteView =
  | { kind: "root" }
  | { kind: "search"; mode: SearchMode }
  | { kind: "suite-tools"; suiteId: string; suiteName: string }
  | {
      kind: "capability-tools";
      itemId: string;
      itemName: string;
      itemKind: CapabilityItem["kind"];
      /// The search mode this view was entered from, so Back returns there.
      fromMode: SearchMode;
    };

const ROOT_VIEW: PaletteView = { kind: "root" };

function viewForLaunchMode(mode: PaletteLaunchMode): PaletteView {
  if (mode === "allResources") return { kind: "search", mode: "all" };
  if (mode === "skills") return { kind: "search", mode: "skill" };
  if (mode === "commands") return { kind: "search", mode: "command" };
  return ROOT_VIEW;
}

interface PaletteState {
  status: Status;
  error: string;
  settings: Settings | null;
  items: CapabilityItem[];
  suites: SuiteDefinition[];
  workspaces: WorkspaceInventoryEntry[];
  /// Live per-tool disk state, keyed by `key(tool, itemId)`. Populated by a
  /// background inspect after the palette is already usable; refreshed on toggle.
  currentMap: Map<string, ToolCapabilityState>;
  /// Suite-managed cells (locked), keyed by `key(tool, itemId)`.
  ownership: Map<string, CapabilityOwnership>;
  /// True once the first background inspect has resolved this summon.
  inspected: boolean;
  view: PaletteView;
  query: string;
  selectedIndex: number;
  results: PaletteItem[];

  load: (launchMode?: PaletteLaunchMode) => Promise<void>;
  setQuery: (query: string) => void;
  move: (delta: number) => void;
  setSelected: (index: number) => void;
  runSelected: () => Promise<void>;
  runSelectedAlt: () => Promise<void>;
  enterMode: (mode: SearchMode) => void;
  enterSuite: (suiteId: string, suiteName: string) => void;
  enterCapabilityTools: (item: CapabilityItem, fromMode: SearchMode) => void;
  toggleCapability: (tool: ToolId, itemId: string) => Promise<void>;
  toggleCapabilityAll: (itemId: string, enable: boolean) => Promise<void>;
  back: () => void;
  reset: () => void;
}

// Route a navigation request back to the main window, then surface it.
function navigate(route: NavRoute) {
  void emitHubNavigate(route).then(() => showMain());
}

// Ask the main window to locate an item (global or workspace), then surface it.
function locate(req: LocateRequest) {
  void emitHubLocate(req).then(() => showMain());
}

// Scan every remembered workspace's inventory so the palette can search across
// all projects. `allSettled` keeps one unreadable project from breaking summon.
async function loadWorkspaces(): Promise<WorkspaceInventoryEntry[]> {
  const { workspaceTargets } = await listWorkspaceTargets();
  const scanned = await Promise.allSettled(
    workspaceTargets.map((target) => scanWorkspace(target.id)),
  );
  return workspaceTargets.flatMap((target, i) => {
    const result = scanned[i];
    return result.status === "fulfilled" ? [{ target, items: result.value.items }] : [];
  });
}

function buildCurrentMap(states: ToolCapabilityState[]): Map<string, ToolCapabilityState> {
  const map = new Map<string, ToolCapabilityState>();
  for (const s of states) map.set(key(s.tool, s.itemId), s);
  return map;
}

function isSettingsManagedItem(item?: CapabilityItem): boolean {
  return item?.sourceId === "agentic-hub";
}

// Suite-managed cells, keyed by `key(tool, itemId)`. A failure must not break
// the panel, so it degrades to an empty (no-lock) map.
async function loadOwnership(): Promise<Map<string, CapabilityOwnership>> {
  const map = new Map<string, CapabilityOwnership>();
  try {
    for (const o of await suiteOwnership()) {
      map.set(key(o.tool, o.itemId), { suiteName: o.suiteName });
    }
  } catch {
    // Leave empty — every tool row stays editable.
  }
  return map;
}

// All callbacks the result builders need, gathered so `recompute` stays a thin
// dispatch over the active view.
interface RecomputeDeps {
  enterMode: (mode: SearchMode) => void;
  enterSuite: (suiteId: string, suiteName: string) => void;
  enterCapabilityTools: (item: CapabilityItem, fromMode: SearchMode) => void;
  toggleCapability: (tool: ToolId, itemId: string) => Promise<void>;
  toggleCapabilityAll: (itemId: string, enable: boolean) => Promise<void>;
}

function recompute(s: PaletteState, deps: RecomputeDeps): PaletteItem[] {
  const { settings, view, query } = s;
  if (!settings) return [];
  if (view.kind === "suite-tools") {
    return computeSuiteToolResults(settings, query, view.suiteId, view.suiteName);
  }
  if (view.kind === "capability-tools") {
    return computeCapabilityToolResults({
      settings,
      query,
      itemId: view.itemId,
      itemName: view.itemName,
      item: s.items.find((it) => it.id === view.itemId),
      inspected: s.inspected,
      currentMap: s.currentMap,
      ownership: s.ownership,
      toggleCapability: deps.toggleCapability,
      toggleCapabilityAll: deps.toggleCapabilityAll,
    });
  }
  const ctx = {
    settings,
    items: s.items,
    suites: s.suites,
    workspaces: s.workspaces,
    query,
    navigate,
    enterMode: deps.enterMode,
    enterSuite: deps.enterSuite,
    enterCapabilityTools: deps.enterCapabilityTools,
    locate,
  };
  return view.kind === "search" ? computeSearchResults(ctx, view.mode) : computeHubResults(ctx);
}

function clamp(index: number, length: number): number {
  if (length === 0) return 0;
  return Math.max(0, Math.min(index, length - 1));
}

export const getInitialState = () => ({
  status: "loading" as Status,
  error: "",
  settings: null,
  items: [],
  suites: [],
  workspaces: [],
  currentMap: new Map<string, ToolCapabilityState>(),
  ownership: new Map<string, CapabilityOwnership>(),
  inspected: false,
  view: ROOT_VIEW,
  query: "",
  selectedIndex: 0,
  results: [],
});

export const usePaletteStore = create<PaletteState>((set, get) => {
  // The in-flight inspect for this summon. Toggle actions await it so they
  // build the full desired map from accurate state (a partial map would wipe
  // every rule/hook not marked enabled — see syncRules/syncHooks).
  let pendingInspect: Promise<void> | null = null;
  let loadGeneration = 0;

  const deps: RecomputeDeps = {
    enterMode: (mode) => enterMode(mode),
    enterSuite: (suiteId, suiteName) => enterSuite(suiteId, suiteName),
    enterCapabilityTools: (item, fromMode) => enterCapabilityTools(item, fromMode),
    toggleCapability: (tool, itemId) => get().toggleCapability(tool, itemId),
    toggleCapabilityAll: (itemId, enable) => get().toggleCapabilityAll(itemId, enable),
  };

  const refreshResults = (overrides: Partial<PaletteState> = {}) => {
    const next = { ...get(), ...overrides };
    set({ ...overrides, selectedIndex: 0, results: recompute(next, deps) });
  };

  // Switch the view, clearing the query so the new level starts clean.
  const enterView = (view: PaletteView) => refreshResults({ view, query: "" });

  const enterMode = (mode: SearchMode) => enterView({ kind: "search", mode });

  const enterSuite = (suiteId: string, suiteName: string) =>
    enterView({ kind: "suite-tools", suiteId, suiteName });

  const enterCapabilityTools = (item: CapabilityItem, fromMode: SearchMode) => {
    enterView({
      kind: "capability-tools",
      itemId: item.id,
      itemName: item.name,
      itemKind: item.kind,
      fromMode,
    });
    // Ensure the per-tool state is loaded; it refreshes the panel when ready.
    void ensureInspected();
  };

  // Inspect every item against the configured tools, plus the suite ownership
  // map, then surface both and re-render. Kept warm across summons so the panel
  // shows last-known state instantly while a fresh inspect lands.
  const runInspect = async () => {
    const { settings, items } = get();
    if (!settings) return;
    try {
      const [result, ownership] = await Promise.all([
        inspect(items, settings.tools),
        loadOwnership(),
      ]);
      refreshResults({ currentMap: buildCurrentMap(result.states), ownership, inspected: true });
    } catch {
      // Leave `inspected` as-is; the panel keeps showing its loading row.
    }
  };

  const ensureInspected = () => {
    if (!pendingInspect) pendingInspect = runInspect();
    return pendingInspect;
  };

  // Apply a desired change for one tool. Builds the COMPLETE desired map from
  // the current state of every item (so syncRules/syncHooks never drop the
  // others), then overlays the requested flips, skipping suite-locked cells.
  const applyToolDesired = async (tool: ToolId, overrides: DesiredMap) => {
    const { items, currentMap, ownership } = get();
    const desiredByItem: DesiredMap = {};
    for (const it of items) {
      desiredByItem[it.id] = currentMap.get(key(tool, it.id))?.state === "enabled";
    }
    for (const [id, value] of Object.entries(overrides)) {
      if (ownership.has(key(tool, id))) continue;
      desiredByItem[id] = value;
    }
    const ops = await plan(tool, items, desiredByItem);
    if (ops.length > 0) await apply(ops);
    await syncRules(tool, items, desiredByItem);
    await syncHooks(tool, items, desiredByItem);
  };

  // Re-inspect to reflect what actually landed, then notify the main window so
  // its matrix refreshes (the file watcher may be paused).
  const reconcile = async () => {
    pendingInspect = runInspect();
    await pendingInspect;
    await emitSourcesChanged();
  };

  return {
    ...getInitialState(),

    load: async (launchMode = "hub") => {
      const generation = ++loadGeneration;
      set({ status: "loading", error: "", inspected: false });
      pendingInspect = null;
      try {
        const settings = await loadSettings();
        const [{ items }, suites, workspaces] = await Promise.all([
          scan(settings.sources),
          listSuites(),
          loadWorkspaces(),
        ]);
        if (generation !== loadGeneration) return;
        refreshResults({
          settings,
          items,
          suites,
          workspaces,
          status: "ready",
          view: viewForLaunchMode(launchMode),
          query: "",
        });
        // Inspect in the background — the palette is already usable for search.
        pendingInspect = runInspect();
        void pendingInspect;
      } catch (e) {
        if (generation !== loadGeneration) return;
        set({ error: messageOf(e), status: "error" });
      }
    },

    setQuery: (query) => refreshResults({ query }),

    // Wraps around both ends so Up from the top lands on the last row.
    move: (delta) => {
      const { results, selectedIndex } = get();
      if (results.length === 0) return;
      const next = (selectedIndex + delta + results.length) % results.length;
      set({ selectedIndex: next });
    },

    setSelected: (index) => set({ selectedIndex: clamp(index, get().results.length) }),

    runSelected: async () => {
      const { results, selectedIndex } = get();
      const item = results[selectedIndex];
      if (!item) return;
      await item.run();
    },

    // Alt+Enter: run the item's alternate action when it has one (resource rows
    // open their source file). Falls back to the primary action otherwise.
    runSelectedAlt: async () => {
      const { results, selectedIndex } = get();
      const item = results[selectedIndex];
      if (!item) return;
      await (item.altRun ?? item.run)();
    },

    enterMode,

    enterSuite,

    enterCapabilityTools,

    // Flip one tool for this item, applied immediately. No-op on locked cells.
    toggleCapability: async (tool, itemId) => {
      await ensureInspected();
      const { currentMap, items, ownership } = get();
      if (isSettingsManagedItem(items.find((it) => it.id === itemId))) return;
      if (ownership.has(key(tool, itemId))) return;
      const on = currentMap.get(key(tool, itemId))?.state === "enabled";
      try {
        await applyToolDesired(tool, { [itemId]: !on });
      } finally {
        await reconcile();
      }
    },

    // Enable/disable this item across every unlocked, projectable enabled tool.
    toggleCapabilityAll: async (itemId, enable) => {
      await ensureInspected();
      const { settings, currentMap, items, ownership } = get();
      if (!settings) return;
      if (isSettingsManagedItem(items.find((it) => it.id === itemId))) return;
      try {
        for (const t of enabledTools(settings)) {
          const k = key(t.id, itemId);
          if (!currentMap.has(k) || ownership.has(k)) continue;
          await applyToolDesired(t.id, { [itemId]: enable });
        }
      } finally {
        await reconcile();
      }
    },

    // Pop one level: capability-tools returns to the search mode it was entered
    // from; suite-tools returns to the suite search mode; a search mode returns
    // to the root hub.
    back: () => {
      const { view } = get();
      let parent: PaletteView = ROOT_VIEW;
      if (view.kind === "suite-tools") parent = { kind: "search", mode: "suite" };
      else if (view.kind === "capability-tools") parent = { kind: "search", mode: view.fromMode };
      refreshResults({ view: parent, query: "" });
    },

    reset: () => refreshResults({ view: ROOT_VIEW, query: "" }),
  };
});
