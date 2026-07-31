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

function suiteRefsEqual(
  left: SuiteCapabilityRef,
  right: SuiteCapabilityRef,
): boolean {
  if (left.cap !== right.cap) return false;
  if (!left.source || !right.source) return !left.source && !right.source;
  return (
    left.source.relHome === right.source.relHome ||
    left.source.folder === right.source.folder
  );
}

export function draftIncludesItem(
  capabilities: readonly SuiteCapabilityRef[],
  item: CapabilityItem,
): boolean {
  return capabilities.some((ref) => suiteRefMatchesItem(ref, item));
}

export interface Draft {
  name: string;
  description: string;
  capabilities: SuiteCapabilityRef[];
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
  setCapabilities: (items: CapabilityItem[], on: boolean) => void;
  removeCapabilities: (refs: SuiteCapabilityRef[]) => void;
  save: () => Promise<void>;
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
    const capabilities: SuiteCapabilityRef[] = items
      .filter(
        (item) =>
          enabledIds.has(item.id) &&
          !readOnlyItemIds.has(item.id) &&
          item.sourceId !== "agentic-hub",
      )
      .map((item) => ({ cap: item.id, source: item.source }));
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
        capabilities: suite.capabilities.map((ref) => ({
          cap: ref.cap,
          source: ref.source ? { ...ref.source } : null,
        })),
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

  setCapabilities: (items, on) => {
    const draft = get().draft;
    if (!draft) return;
    let capabilities = [...draft.capabilities];
    for (const item of items) {
      if (on) {
        if (!draftIncludesItem(capabilities, item)) {
          capabilities.push({ cap: item.id, source: item.source });
        }
      } else {
        capabilities = capabilities.filter(
          (ref) => !suiteRefMatchesItem(ref, item),
        );
      }
    }
    set({ draft: { ...draft, capabilities } });
  },

  removeCapabilities: (refs) => {
    const draft = get().draft;
    if (!draft) return;
    set({
      draft: {
        ...draft,
        capabilities: draft.capabilities.filter(
          (capability) => !refs.some((ref) => suiteRefsEqual(capability, ref)),
        ),
      },
    });
  },

  save: async () => {
    const { draft, isCreating, selectedId, reload, selectSuite } = get();
    if (!draft || !draft.name.trim()) return;
    set({ busy: true });
    try {
      const payload = {
        name: draft.name.trim(),
        description: draft.description.trim() || null,
        capabilities: draft.capabilities,
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
