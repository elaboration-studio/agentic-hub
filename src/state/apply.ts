// Shared suite-apply flow: preview extras, confirm preserve vs full reset, toast.
import { create } from "zustand";
import { toast } from "sonner";
import { applySuite, suiteApplyPreview } from "../ipc";
import type { ApplySuiteResult, ToolId } from "../types";
import { messageOf } from "../shared";

export interface ApplyPending {
  tool: ToolId;
  suiteId: string;
  suiteName: string;
  extras: string[];
}

interface ApplyState {
  pending: ApplyPending | null;
  busy: boolean;
  request: (tool: ToolId, suiteId: string, suiteName: string) => Promise<void>;
  confirm: (preserve: boolean) => Promise<void>;
  cancel: () => void;
  runApply: (tool: ToolId, suiteId: string, preserveManual: boolean) => Promise<void>;
}

function toastApplyResult(tool: ToolId, result: ApplySuiteResult) {
  const ar = result.applyResult;
  const errors = ar.errors.length + result.ruleSync.errors.length + result.hookSync.errors.length;
  const parts = [`${ar.created} added`, `${ar.removed} removed`];
  if (result.skippedStale > 0) parts.push(`${result.skippedStale} stale skipped`);
  if (result.skippedAbsentSource > 0) {
    parts.push(`${result.skippedAbsentSource} from sources not on this machine, preserved`);
  }
  if (errors > 0) parts.push(`${errors} error(s)`);
  const msg = `Applied to ${tool} · ${parts.join(", ")}`;
  if (errors > 0) toast.warning(msg);
  else toast.success(msg);
}

export const useApplyStore = create<ApplyState>((set, get) => ({
  pending: null,
  busy: false,

  request: async (tool, suiteId, suiteName) => {
    set({ busy: true });
    try {
      const extras = await suiteApplyPreview(tool, suiteId);
      if (extras.length === 0) {
        await get().runApply(tool, suiteId, false);
        return;
      }
      set({ pending: { tool, suiteId, suiteName, extras }, busy: false });
    } catch (e) {
      toast.error(messageOf(e));
      set({ busy: false });
    }
  },

  confirm: async (preserve) => {
    const p = get().pending;
    if (!p) return;
    set({ pending: null });
    await get().runApply(p.tool, p.suiteId, preserve);
  },

  cancel: () => set({ pending: null }),

  runApply: async (tool, suiteId, preserveManual) => {
    set({ busy: true });
    try {
      const result = await applySuite(tool, suiteId, preserveManual);
      toastApplyResult(tool, result);
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ busy: false });
    }
  },
}));
