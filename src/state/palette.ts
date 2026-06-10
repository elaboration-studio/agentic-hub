// Command-palette state: load resources + suites + settings once per summon,
// hold the query and selection, and derive the visible result list. The
// palette is layered: a `root` hub of first-class commands, a `search` view
// per drilled-in mode (kind search or locate scope), and a `suite-tools` view
// reached through the suite mode (pick a tool to apply it to). Logic lives in
// the store (not the component) so the matching, navigation, and selection are
// unit-testable without a DOM.

import { create } from "zustand";
import {
  emitHubLocate,
  emitHubNavigate,
  listSuites,
  listWorkspaceTargets,
  loadSettings,
  scan,
  scanWorkspace,
  showMain,
  type LocateRequest,
  type NavRoute,
} from "../ipc";
import type { CapabilityItem, Settings, SuiteDefinition } from "../types";
import { messageOf } from "../shared";
import {
  computeHubResults,
  computeSearchResults,
  computeSuiteToolResults,
  type PaletteItem,
  type SearchMode,
  type WorkspaceInventoryEntry,
} from "../components/palette/commands";

export type Status = "loading" | "ready" | "error";

/// Which level of the palette is showing.
export type PaletteView =
  | { kind: "root" }
  | { kind: "search"; mode: SearchMode }
  | { kind: "suite-tools"; suiteId: string; suiteName: string };

const ROOT_VIEW: PaletteView = { kind: "root" };

interface PaletteState {
  status: Status;
  error: string;
  settings: Settings | null;
  items: CapabilityItem[];
  suites: SuiteDefinition[];
  workspaces: WorkspaceInventoryEntry[];
  view: PaletteView;
  query: string;
  selectedIndex: number;
  results: PaletteItem[];

  load: () => Promise<void>;
  setQuery: (query: string) => void;
  move: (delta: number) => void;
  setSelected: (index: number) => void;
  runSelected: () => Promise<void>;
  runSelectedAlt: () => Promise<void>;
  enterMode: (mode: SearchMode) => void;
  enterSuite: (suiteId: string, suiteName: string) => void;
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

function recompute(
  settings: Settings | null,
  items: CapabilityItem[],
  suites: SuiteDefinition[],
  workspaces: WorkspaceInventoryEntry[],
  view: PaletteView,
  query: string,
  enterMode: (mode: SearchMode) => void,
  enterSuite: (suiteId: string, suiteName: string) => void,
): PaletteItem[] {
  if (!settings) return [];
  if (view.kind === "suite-tools") {
    return computeSuiteToolResults(settings, query, view.suiteId, view.suiteName);
  }
  const ctx = {
    settings,
    items,
    suites,
    workspaces,
    query,
    navigate,
    enterMode,
    enterSuite,
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
  view: ROOT_VIEW,
  query: "",
  selectedIndex: 0,
  results: [],
});

export const usePaletteStore = create<PaletteState>((set, get) => {
  // Stable drill-in callbacks for the provider context: switch the view,
  // clearing the query so the new level starts clean.
  const enterView = (view: PaletteView) => {
    const { settings, items, suites, workspaces } = get();
    set({
      view,
      query: "",
      selectedIndex: 0,
      results: recompute(settings, items, suites, workspaces, view, "", enterMode, enterSuite),
    });
  };

  const enterMode = (mode: SearchMode) => enterView({ kind: "search", mode });

  const enterSuite = (suiteId: string, suiteName: string) =>
    enterView({ kind: "suite-tools", suiteId, suiteName });

  const refreshResults = (overrides: Partial<PaletteState> = {}) => {
    const next = { ...get(), ...overrides };
    set({
      ...overrides,
      selectedIndex: 0,
      results: recompute(
        next.settings,
        next.items,
        next.suites,
        next.workspaces,
        next.view,
        next.query,
        enterMode,
        enterSuite,
      ),
    });
  };

  return {
    ...getInitialState(),

    load: async () => {
      set({ status: "loading", error: "" });
      try {
        const settings = await loadSettings();
        const [{ items }, suites, workspaces] = await Promise.all([
          scan(settings.sources),
          listSuites(),
          loadWorkspaces(),
        ]);
        refreshResults({ settings, items, suites, workspaces, status: "ready" });
      } catch (e) {
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

    // Alt+Enter: run the item's alternate action when it has one (commands open
    // their source file). Falls back to the primary action otherwise.
    runSelectedAlt: async () => {
      const { results, selectedIndex } = get();
      const item = results[selectedIndex];
      if (!item) return;
      await (item.altRun ?? item.run)();
    },

    enterMode,

    enterSuite,

    // Pop one level: suite-tools returns to the suite search mode it was
    // entered from; a search mode returns to the root hub.
    back: () => {
      const { view } = get();
      const parent: PaletteView =
        view.kind === "suite-tools" ? { kind: "search", mode: "suite" } : ROOT_VIEW;
      refreshResults({ view: parent, query: "" });
    },

    reset: () => refreshResults({ view: ROOT_VIEW, query: "" }),
  };
});
