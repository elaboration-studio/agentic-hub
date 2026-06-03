// Command-palette state: load resources + settings once per summon, hold the
// query and selection, and derive the visible result list from the command
// registry. Lives in a store (not the component) so the matching/selection
// logic is unit-testable without a DOM. Mirrors the manager store's pattern of
// recomputing derived values inside mutating actions.

import { create } from "zustand";
import { emitHubNavigate, loadSettings, scan, showMain, type NavRoute } from "../ipc";
import type { CapabilityItem, Settings } from "../types";
import { messageOf } from "../shared";
import { computeResults, type PaletteItem } from "../components/palette/commands";

export type Status = "loading" | "ready" | "error";

interface PaletteState {
  status: Status;
  error: string;
  settings: Settings | null;
  items: CapabilityItem[];
  query: string;
  selectedIndex: number;
  results: PaletteItem[];

  load: () => Promise<void>;
  setQuery: (query: string) => void;
  move: (delta: number) => void;
  setSelected: (index: number) => void;
  runSelected: () => Promise<void>;
  reset: () => void;
}

// Route a navigation request back to the main window, then surface it.
function navigate(route: NavRoute) {
  void emitHubNavigate(route).then(() => showMain());
}

function recompute(settings: Settings | null, items: CapabilityItem[], query: string): PaletteItem[] {
  if (!settings) return [];
  return computeResults({ settings, items, query, navigate });
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
  query: "",
  selectedIndex: 0,
  results: [],
});

export const usePaletteStore = create<PaletteState>((set, get) => ({
  ...getInitialState(),

  load: async () => {
    set({ status: "loading", error: "" });
    try {
      const settings = await loadSettings();
      const { items } = await scan(settings.sources);
      set({
        settings,
        items,
        results: recompute(settings, items, get().query),
        selectedIndex: 0,
        status: "ready",
      });
    } catch (e) {
      set({ error: messageOf(e), status: "error" });
    }
  },

  setQuery: (query) => {
    const { settings, items } = get();
    set({ query, results: recompute(settings, items, query), selectedIndex: 0 });
  },

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

  reset: () => set({ query: "", selectedIndex: 0, results: recompute(get().settings, get().items, "") }),
}));
