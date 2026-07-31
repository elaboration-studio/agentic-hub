// Workspace-scope state: the remembered project dirs (left rail) and the active
// one. Selecting a workspace loads its read-only inventory into the manager
// store, which reuses the global matrix render. No suites, no apply — workspace
// scope is a read-only audit. Errors surface as toasts.

import { create } from "zustand";
import { toast } from "sonner";
import {
  listWorkspaceTargets,
  pickWorkspaceDir,
  removeWorkspaceTarget,
  setActiveWorkspaceTarget,
} from "../ipc";
import type { WorkspaceTarget } from "../types";
import { messageOf } from "../shared";
import { useManagerStore } from "./manager";

interface WorkspaceState {
  targets: WorkspaceTarget[];
  activeId: string;
  busy: boolean;

  reload: () => Promise<void>;
  pick: () => Promise<boolean>;
  activate: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

/// Load the active workspace's inventory into the manager store, when one
/// exists. Centralizes the manager handoff so every mutation refreshes the view.
async function loadActive(id: string): Promise<void> {
  if (id) await useManagerStore.getState().loadWorkspace(id);
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  targets: [],
  activeId: "",
  busy: false,

  reload: async () => {
    try {
      const state = await listWorkspaceTargets();
      const activeId = state.workspaceActiveId ?? state.workspaceTargets[0]?.id ?? "";
      set({ targets: state.workspaceTargets, activeId });
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  pick: async () => {
    set({ busy: true });
    try {
      const target = await pickWorkspaceDir();
      await get().reload();
      set({ activeId: target.id });
      await loadActive(target.id);
      return true;
    } catch (e) {
      const msg = messageOf(e);
      if (!msg.includes("No folder selected")) toast.error(msg);
      return false;
    } finally {
      set({ busy: false });
    }
  },

  activate: async (id) => {
    set({ activeId: id });
    try {
      await setActiveWorkspaceTarget(id);
      await loadActive(id);
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  remove: async (id) => {
    try {
      await removeWorkspaceTarget(id);
      await get().reload();
      await loadActive(get().activeId);
    } catch (e) {
      toast.error(messageOf(e));
    }
  },
}));
