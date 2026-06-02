// Workspace-scope state, extracted from WorkspacePanel: target dirs, the active
// target, suite/tool selection, and the hard-copy apply. Errors surface as
// toasts.

import { create } from "zustand";
import { toast } from "sonner";
import {
  applyWorkspacePatch,
  listSuites,
  listWorkspaceTargets,
  pickWorkspaceDir,
  removeWorkspaceTarget,
  setActiveWorkspaceTarget,
} from "../ipc";
import type {
  SuiteDefinition,
  ToolId,
  WorkspacePatchResult,
  WorkspaceTarget,
} from "../types";
import { messageOf } from "../shared";

interface WorkspaceState {
  targets: WorkspaceTarget[];
  activeId: string;
  suites: SuiteDefinition[];
  suiteId: string;
  tool: ToolId;
  busy: boolean;
  result: WorkspacePatchResult | null;

  reload: () => Promise<void>;
  pick: () => Promise<void>;
  activate: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  apply: () => Promise<void>;
  setSuiteId: (id: string) => void;
  setTool: (tool: ToolId) => void;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  targets: [],
  activeId: "",
  suites: [],
  suiteId: "",
  tool: "codex",
  busy: false,
  result: null,

  reload: async () => {
    try {
      const [state, suiteList] = await Promise.all([listWorkspaceTargets(), listSuites()]);
      const activeId = state.workspaceActiveId ?? state.workspaceTargets[0]?.id ?? "";
      const cur = get().suiteId;
      const suiteId = suiteList.some((s) => s.id === cur) ? cur : (suiteList[0]?.id ?? "");
      set({ targets: state.workspaceTargets, activeId, suites: suiteList, suiteId });
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
    } catch (e) {
      const msg = messageOf(e);
      if (!msg.includes("No folder selected")) toast.error(msg);
    } finally {
      set({ busy: false });
    }
  },

  activate: async (id) => {
    set({ activeId: id });
    try {
      await setActiveWorkspaceTarget(id);
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  remove: async (id) => {
    try {
      await removeWorkspaceTarget(id);
      await get().reload();
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  apply: async () => {
    const { activeId, suiteId, tool } = get();
    if (!activeId || !suiteId) return;
    set({ busy: true, result: null });
    try {
      const res = await applyWorkspacePatch(activeId, tool, suiteId);
      set({ result: res });
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },

  setSuiteId: (id) => set({ suiteId: id }),
  setTool: (tool) => set({ tool }),
}));
