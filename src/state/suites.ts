// Suite CRUD + draft editing, extracted from SuitesPage. View-only filters
// (search, kind filter, tree collapse) stay local to the component. Action
// errors surface as toasts so the panel needs no onError prop.

import { create } from "zustand";
import { toast } from "sonner";
import {
  createSuite,
  deleteSuite,
  listSuites,
  setBaseSuite,
  updateSuite,
} from "../ipc";
import type {
  CapabilityItem,
  SuiteCapabilityRef,
  SuiteDefinition,
  ToolCapabilityState,
  ToolId,
} from "../types";
import { messageOf } from "../shared";

function isSuiteSelectableItem(item: CapabilityItem | undefined): boolean {
  return item?.sourceId !== "agentic-hub";
}

export function suiteRefMatchesItem(
  ref: SuiteCapabilityRef,
  item: CapabilityItem,
): boolean {
  return (
    ref.cap === item.id &&
    (!ref.source ||
      ref.source.relHome === item.source.relHome ||
      ref.source.folder === item.source.folder)
  );
}

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
  startCreateFromCurrent: (
    tool: ToolId,
    items: CapabilityItem[],
    states: ToolCapabilityState[],
    readOnlyItemIds: ReadonlySet<string>,
  ) => void;
  selectSuite: (id: string) => void;
  cancelEdit: () => void;
  setDraft: (patch: Partial<Draft>) => void;
  setCapabilities: (ids: string[], on: boolean) => void;
  removeCapabilities: (ids: string[]) => void;
  save: (items: CapabilityItem[]) => Promise<void>;
  remove: () => Promise<void>;
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

  startCreateFromCurrent: (tool, items, states, readOnlyItemIds) => {
    const enabledIds = new Set(
      states
        .filter((state) => state.tool === tool && state.state === "enabled")
        .map((state) => state.itemId),
    );
    const capabilities = items
      .filter(
        (item) =>
          enabledIds.has(item.id) &&
          !readOnlyItemIds.has(item.id) &&
          item.sourceId !== "agentic-hub",
      )
      .map((item) => item.id);
    set({
      isCreating: true,
      selectedId: undefined,
      draft: { name: "", description: "", capabilities },
    });
  },

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

  removeCapabilities: (ids) => {
    const draft = get().draft;
    if (!draft) return;
    const removed = new Set(ids);
    set({
      draft: {
        ...draft,
        capabilities: draft.capabilities.filter((capability) => !removed.has(capability)),
      },
    });
  },

  save: async (items) => {
    const { draft, isCreating, selectedId, reload, selectSuite, suites } = get();
    if (!draft || !draft.name.trim()) return;
    set({ busy: true });
    try {
      // Attach each selected id's live source. A missing reference keeps its
      // original portable source identity until the user explicitly removes it.
      const itemById = new Map(items.map((it) => [it.id, it]));
      const originalById = new Map(
        suites
          .find((suite) => suite.id === selectedId)
          ?.capabilities.map((ref) => [ref.cap, ref]) ?? [],
      );
      const capabilities: SuiteCapabilityRef[] = draft.capabilities
        .filter((cap) => originalById.has(cap) || isSuiteSelectableItem(itemById.get(cap)))
        .map((cap) => ({
          cap,
          source: originalById.get(cap)?.source ?? itemById.get(cap)?.source ?? null,
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
