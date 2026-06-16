// CLI-tools preflight state: the tool catalog (bundled + optional user
// override, served by Rust) and the per-tool probe results. Probing shells out
// in Rust; each row updates independently as its check resolves, so a slow
// auth check never blocks the rest. Errors surface as toasts.

import { create } from "zustand";
import { toast } from "sonner";
import { checkTool, listToolCatalog } from "../ipc";
import type { CliTool, CliToolStatus } from "../types";
import { messageOf } from "../shared";

interface CliToolsState {
  catalog: CliTool[];
  statusById: Record<string, CliToolStatus>;
  /// Ids whose probe is in flight (drives the per-row spinner).
  checking: Set<string>;
  loading: boolean;
  loaded: boolean;

  /// Resolves `true` when the catalog loaded, so callers can gate follow-up
  /// work (probing) on a successful load and offer a retry on failure.
  loadCatalog: () => Promise<boolean>;
  checkOne: (id: string) => Promise<void>;
  checkAll: () => Promise<void>;
}

export const useCliToolsStore = create<CliToolsState>((set, get) => ({
  catalog: [],
  statusById: {},
  checking: new Set(),
  loading: false,
  loaded: false,

  loadCatalog: async () => {
    set({ loading: true });
    try {
      const catalog = await listToolCatalog();
      set({ catalog, loaded: true });
      return true;
    } catch (e) {
      toast.error(messageOf(e));
      return false;
    } finally {
      set({ loading: false });
    }
  },

  checkOne: async (id) => {
    set((s) => ({ checking: new Set(s.checking).add(id) }));
    try {
      const status = await checkTool(id);
      set((s) => ({ statusById: { ...s.statusById, [id]: status } }));
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set((s) => {
        const next = new Set(s.checking);
        next.delete(id);
        return { checking: next };
      });
    }
  },

  checkAll: async () => {
    const { catalog, checkOne } = get();
    await Promise.all(catalog.map((t) => checkOne(t.id)));
  },
}));
