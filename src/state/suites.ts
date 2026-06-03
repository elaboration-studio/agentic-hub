// Suite CRUD + draft editing, extracted from SuitesPage. View-only filters
// (search, kind filter, tree collapse) stay local to the component. Action
// errors surface as toasts so the panel needs no onError prop.

import { create } from "zustand";
import { toast } from "sonner";
import {
  applySuite,
  createSuite,
  deleteSuite,
  listSuites,
  setBaseSuite,
  updateSuite,
} from "../ipc";
import type { CapabilityItem, SuiteCapabilityRef, SuiteDefinition, ToolId } from "../types";
import { messageOf } from "../shared";

export interface Draft {
  name: string;
  description: string;
  // Bare capability ids, for the checkbox tree. Source is re-attached on save
  // from the live scan, so saved suites are portable across devices.
  capabilities: string[];
}

interface SuitesState {
  suites: SuiteDefinition[];
  selectedId: string | undefined;
  isCreating: boolean;
  draft: Draft | undefined;
  busy: boolean;

  reload: () => Promise<void>;
  startCreate: () => void;
  selectSuite: (id: string) => void;
  cancelEdit: () => void;
  setDraft: (patch: Partial<Draft>) => void;
  setCapabilities: (ids: string[], on: boolean) => void;
  save: (items: CapabilityItem[]) => Promise<void>;
  remove: () => Promise<void>;
  applySelected: (tool: ToolId) => Promise<void>;
  setBase: (id: string | null) => Promise<void>;
  pruneSelection: () => void;
}

export const useSuitesStore = create<SuitesState>((set, get) => ({
  suites: [],
  selectedId: undefined,
  isCreating: false,
  draft: undefined,
  busy: false,

  reload: async () => {
    try {
      set({ suites: await listSuites() });
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  startCreate: () =>
    set({
      isCreating: true,
      selectedId: undefined,
      draft: { name: "", description: "", capabilities: [] },
    }),

  selectSuite: (id) => {
    const suite = get().suites.find((s) => s.id === id);
    if (!suite) return;
    set({
      isCreating: false,
      selectedId: id,
      draft: {
        name: suite.name,
        description: suite.description ?? "",
        capabilities: suite.capabilities.map((r) => r.cap),
      },
    });
  },

  cancelEdit: () => {
    const { isCreating, selectedId, selectSuite } = get();
    if (isCreating) {
      set({ isCreating: false, draft: undefined });
    } else if (selectedId) {
      selectSuite(selectedId);
    } else {
      set({ draft: undefined });
    }
  },

  setDraft: (patch) => {
    const draft = get().draft;
    if (!draft) return;
    set({ draft: { ...draft, ...patch } });
  },

  setCapabilities: (ids, on) => {
    const draft = get().draft;
    if (!draft) return;
    const next = new Set(draft.capabilities);
    for (const id of ids) {
      if (on) next.add(id);
      else next.delete(id);
    }
    set({ draft: { ...draft, capabilities: [...next] } });
  },

  save: async (items) => {
    const { draft, isCreating, selectedId, reload, selectSuite } = get();
    if (!draft || !draft.name.trim()) return;
    set({ busy: true });
    try {
      // Attach each selected id's source from the live scan so the saved suite
      // is portable. Ids absent from the scan stay unqualified (source: null).
      const sourceOf = new Map(items.map((it) => [it.id, it.source]));
      const capabilities: SuiteCapabilityRef[] = draft.capabilities.map((cap) => ({
        cap,
        source: sourceOf.get(cap) ?? null,
      }));
      const payload = {
        name: draft.name.trim(),
        description: draft.description.trim() || null,
        capabilities,
      };
      if (isCreating) {
        const created = await createSuite(payload);
        set({ isCreating: false });
        await reload();
        selectSuite(created.id);
      } else if (selectedId) {
        await updateSuite(selectedId, payload);
        await reload();
      }
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },

  remove: async () => {
    const { selectedId, reload } = get();
    if (!selectedId) return;
    set({ busy: true });
    try {
      await deleteSuite(selectedId);
      set({ selectedId: undefined, draft: undefined });
      await reload();
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },

  applySelected: async (tool) => {
    const { selectedId } = get();
    if (!selectedId) return;
    set({ busy: true });
    try {
      const result = await applySuite(tool, selectedId);
      const ar = result.applyResult;
      const parts = [`${ar.created} added`, `${ar.removed} removed`];
      if (result.skippedStale > 0) parts.push(`${result.skippedStale} stale skipped`);
      if (result.skippedAbsentSource > 0)
        parts.push(`${result.skippedAbsentSource} from sources not on this machine, preserved`);
      if (ar.errors.length > 0) parts.push(`${ar.errors.length} error(s)`);
      const msg = `Applied to ${tool} · ${parts.join(", ")}`;
      if (ar.errors.length > 0) toast.warning(msg);
      else toast.success(msg);
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },

  setBase: async (id) => {
    set({ busy: true });
    try {
      await setBaseSuite(id);
      await get().reload();
      toast.success(id ? "Marked as base suite" : "Base suite cleared");
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },

  // Drop selection if the selected suite vanished (deleted elsewhere).
  pruneSelection: () => {
    const { suites, selectedId, isCreating } = get();
    if (selectedId && !suites.some((s) => s.id === selectedId)) {
      set({ selectedId: undefined, draft: isCreating ? get().draft : undefined });
    }
  },
}));
