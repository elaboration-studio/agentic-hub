// Session-scoped filter state for the capability matrix. Lives in a store
// instead of component state so search, type/source filters, the flat/tree
// view, the "enabled only" toggle, and collapsed folders all survive route and
// scope switches (the Matrix component unmounts on those). In-memory only —
// filters reset on app restart by design.

import { create } from "zustand";
import type { KindFilter, View } from "../shared";

interface ManagerFiltersState {
  view: View;
  query: string;
  source: string;
  kind: KindFilter;
  enabledOnly: boolean;
  collapsed: Set<string>;

  setView: (view: View) => void;
  setQuery: (query: string) => void;
  setSource: (source: string) => void;
  setKind: (kind: KindFilter) => void;
  setEnabledOnly: (enabledOnly: boolean) => void;
  setCollapsed: (collapsed: Set<string>) => void;
  toggleCollapsed: (path: string) => void;
}

export const useManagerFiltersStore = create<ManagerFiltersState>((set) => ({
  view: "tree",
  query: "",
  source: "",
  kind: "all",
  enabledOnly: false,
  collapsed: new Set<string>(),

  setView: (view) => set({ view }),
  setQuery: (query) => set({ query }),
  setSource: (source) => set({ source }),
  setKind: (kind) => set({ kind }),
  setEnabledOnly: (enabledOnly) => set({ enabledOnly }),
  setCollapsed: (collapsed) => set({ collapsed }),
  toggleCollapsed: (path) =>
    set((s) => {
      const next = new Set(s.collapsed);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return { collapsed: next };
    }),
}));
