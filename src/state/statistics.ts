// Statistics dashboard: loads scan + usage dashboard aggregates over IPC.
// View-only date-range state lives here; charts read derived dashboard data.

import { create } from "zustand";
import { toast } from "sonner";
import { queryUsageDashboard, scan, usageTracingStatus } from "@/ipc";
import type { UsageDashboard, UsageDateRange, UsageTracingStatus } from "@/types";
import { messageOf } from "@/shared";
import { useManagerStore } from "./manager";

export const DEFAULT_USAGE_RANGE: UsageDateRange = "last30Days";

interface StatisticsState {
  dashboard: UsageDashboard | null;
  range: UsageDateRange;
  tracingStatus: UsageTracingStatus | null;
  loading: boolean;
  error: string | null;

  setRange: (range: UsageDateRange) => void;
  reload: () => Promise<void>;
}

export const useStatisticsStore = create<StatisticsState>((set, get) => ({
  dashboard: null,
  range: DEFAULT_USAGE_RANGE,
  tracingStatus: null,
  loading: false,
  error: null,

  setRange: (range) => {
    set({ range });
    void get().reload();
  },

  reload: async () => {
    const sources = useManagerStore.getState().data?.settings.sources;
    if (!sources) return;

    set({ loading: true, error: null });
    try {
      const [scanResult, tracingStatus] = await Promise.all([
        scan(sources),
        usageTracingStatus(),
      ]);
      const dashboard = tracingStatus.enabled
        ? await queryUsageDashboard(scanResult.items, get().range)
        : null;
      set({ dashboard, tracingStatus, loading: false });
    } catch (e) {
      const message = messageOf(e);
      set({ error: message, loading: false });
      toast.error(message);
    }
  },
}));
