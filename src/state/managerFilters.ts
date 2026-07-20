// Session-scoped filter state for the capability matrix. Lives in a store
// instead of component state so search, type/source filters, the flat/tree
// view, the "enabled only" toggle, and collapsed folders all survive route and
// scope switches (the Matrix component unmounts on those). In-memory only —
// filters reset on app restart by design.

import { create } from "zustand";
import type { KindFilter, UsageSort, View } from "../shared";

interface ManagerFiltersState {
  view: View;
  query: string;
  source: string;
  kind: KindFilter;
  enabledOnly: boolean;
  usageSort: UsageSort;
  collapsed: Set<string>;
  // The matrix row to surface (namespaced item id), set by a palette locate.
  // Empty when nothing is being located. The Matrix clears it after scrolling.
  locateId: string;

  setView: (view: View) => void;
  setQuery: (query: string) => void;
  setSource: (source: string) => void;
  setKind: (kind: KindFilter) => void;
  setEnabledOnly: (enabledOnly: boolean) => void;
  setUsageSort: (usageSort: UsageSort) => void;
  setCollapsed: (collapsed: Set<string>) => void;
  toggleCollapsed: (path: string) => void;
  setLocate: (id: string) => void;
  clearLocate: () => void;
  // Reset only the scope-specific filters (source, enabledOnly, collapsed,
  // locateId) when switching between global and workspace. The universal
  // query/kind/view carry over since they mean the same thing in both scopes.
  resetScopedFilters: () => void;
}

export const useManagerFiltersStore = create<ManagerFiltersState>((set) => ({
  view: "tree",
  query: "",
  source: "",
  kind: "all",
  enabledOnly: false,
  usageSort: "lastUsed",
  collapsed: new Set<string>(),
  locateId: "",

  setView: (view) => set({ view }),
  setQuery: (query) => set({ query }),
  setSource: (source) => set({ source }),
  setKind: (kind) => set({ kind }),
  setEnabledOnly: (enabledOnly) => set({ enabledOnly }),
  setUsageSort: (usageSort) => set({ usageSort }),
  setCollapsed: (collapsed) => set({ collapsed }),
  toggleCollapsed: (path) =>
    set((s) => {
      const next = new Set(s.collapsed);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return { collapsed: next };
    }),
  setLocate: (id) => set({ locateId: id }),
  clearLocate: () => set({ locateId: "" }),
  resetScopedFilters: () =>
    set({ source: "", enabledOnly: false, collapsed: new Set<string>(), locateId: "" }),
}));
