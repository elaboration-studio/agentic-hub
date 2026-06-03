// Command-palette state: load resources + suites + settings once per summon,
// hold the query and selection, and derive the visible result list. The
// palette is two-level: a `root` view (search resources/suites/nav) and a
// `suite-tools` view reached by drilling into a suite (pick a tool to apply
// it to). Logic lives in the store (not the component) so the matching,
// navigation, and selection are unit-testable without a DOM.

import { create } from "zustand";
import {
  emitHubNavigate,
  listSuites,
  loadSettings,
  scan,
  showMain,
  type NavRoute,
} from "../ipc";
import type { CapabilityItem, Settings, SuiteDefinition } from "../types";
import { messageOf } from "../shared";
import {
  computeResults,
  computeSuiteToolResults,
  type PaletteItem,
} from "../components/palette/commands";

export type Status = "loading" | "ready" | "error";

/// Which level of the palette is showing.
export type PaletteView =
  | { kind: "root" }
  | { kind: "suite-tools"; suiteId: string; suiteName: string };

const ROOT_VIEW: PaletteView = { kind: "root" };

interface PaletteState {
  status: Status;
  error: string;
  settings: Settings | null;
  items: CapabilityItem[];
  suites: SuiteDefinition[];
  view: PaletteView;
  query: string;
  selectedIndex: number;
  results: PaletteItem[];

  load: () => Promise<void>;
  setQuery: (query: string) => void;
  move: (delta: number) => void;
  setSelected: (index: number) => void;
  runSelected: () => Promise<void>;
  enterSuite: (suiteId: string, suiteName: string) => void;
  back: () => void;
  reset: () => void;
}

// Route a navigation request back to the main window, then surface it.
function navigate(route: NavRoute) {
  void emitHubNavigate(route).then(() => showMain());
}

function recompute(
  settings: Settings | null,
  items: CapabilityItem[],
  suites: SuiteDefinition[],
  view: PaletteView,
  query: string,
  enterSuite: (suiteId: string, suiteName: string) => void,
): PaletteItem[] {
  if (!settings) return [];
  if (view.kind === "suite-tools") {
    return computeSuiteToolResults(settings, query, view.suiteId, view.suiteName);
  }
  return computeResults({ settings, items, suites, query, navigate, enterSuite });
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
  view: ROOT_VIEW,
  query: "",
  selectedIndex: 0,
  results: [],
});

export const usePaletteStore = create<PaletteState>((set, get) => {
  // A stable enterSuite for the provider context: switch to the suite-tools
  // view, clearing the query so the tool list shows in full.
  const enterSuite = (suiteId: string, suiteName: string) => {
    const { settings, items, suites } = get();
    const view: PaletteView = { kind: "suite-tools", suiteId, suiteName };
    set({
      view,
      query: "",
      selectedIndex: 0,
      results: recompute(settings, items, suites, view, "", enterSuite),
    });
  };

  const refreshResults = (overrides: Partial<PaletteState> = {}) => {
    const next = { ...get(), ...overrides };
    set({
      ...overrides,
      selectedIndex: 0,
      results: recompute(next.settings, next.items, next.suites, next.view, next.query, enterSuite),
    });
  };

  return {
    ...getInitialState(),

    load: async () => {
      set({ status: "loading", error: "" });
      try {
        const settings = await loadSettings();
        const [{ items }, suites] = await Promise.all([scan(settings.sources), listSuites()]);
        refreshResults({ settings, items, suites, status: "ready" });
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

    enterSuite,

    // Return from the suite-tools view to the root, clearing the query.
    back: () => refreshResults({ view: ROOT_VIEW, query: "" }),

    reset: () => refreshResults({ view: ROOT_VIEW, query: "" }),
  };
});
