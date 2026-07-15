// Statistics dashboard: resource inventory + usage aggregates over IPC.
// Inventory is always computed; usage dashboard is gated on tracing.

import { create } from "zustand";
import { toast } from "sonner";
import { listSkillFavorites, loadSettings, queryUsageDashboard, scan, usageTracingStatus } from "@/ipc";
import type {
  CapabilityItem,
  CapabilityKind,
  Settings,
  SkillFavorite,
  UsageDashboard,
  UsageDateRange,
  UsageTracingStatus,
} from "@/types";
import { ALL_TOOLS, enabledTools, messageOf } from "@/shared";
import { useManagerStore } from "./manager";

export const DEFAULT_USAGE_RANGE: UsageDateRange = "last30Days";

export interface ResourceInventory {
  total: number;
  sourceCount: number;
  enabledTools: number;
  totalTools: number;
  favoritesCount: number;
  byKind: Record<CapabilityKind, number>;
}

function emptyByKind(): Record<CapabilityKind, number> {
  return { skill: 0, agent: 0, rule: 0, hook: 0, command: 0 };
}

export function computeResourceInventory(
  items: CapabilityItem[],
  settings: Settings,
  favorites: SkillFavorite[],
): ResourceInventory {
  const byKind = emptyByKind();
  for (const item of items) {
    byKind[item.kind] += 1;
  }
  return {
    total: items.length,
    sourceCount: settings.sources.length,
    enabledTools: enabledTools(settings).length,
    totalTools: ALL_TOOLS.length,
    favoritesCount: favorites.length,
    byKind,
  };
}

interface StatisticsState {
  inventory: ResourceInventory | null;
  dashboard: UsageDashboard | null;
  range: UsageDateRange;
  tracingStatus: UsageTracingStatus | null;
  loading: boolean;
  error: string | null;

  setRange: (range: UsageDateRange) => void;
  reload: () => Promise<void>;
}

async function resolveManagerData(): Promise<{
  items: CapabilityItem[];
  settings: Settings;
} | null> {
  const manager = useManagerStore.getState();
  let data = manager.data;
  if (!data) {
    await manager.refresh();
    data = useManagerStore.getState().data;
  }
  if (data) {
    return { items: data.items, settings: data.settings };
  }

  const settings = await loadSettings();
  const scanResult = await scan(settings.sources);
  return { items: scanResult.items, settings };
}

export const useStatisticsStore = create<StatisticsState>((set, get) => ({
  inventory: null,
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
    set({ loading: true, error: null });
    try {
      const [managerData, tracingStatus, favoritesState] = await Promise.all([
        resolveManagerData(),
        usageTracingStatus(),
        listSkillFavorites(),
      ]);
      if (!managerData) {
        set({ loading: false });
        return;
      }

      const inventory = computeResourceInventory(
        managerData.items,
        managerData.settings,
        favoritesState.favorites,
      );
      const dashboard = tracingStatus.enabled
        ? await queryUsageDashboard(managerData.items, get().range)
        : null;
      set({ inventory, dashboard, tracingStatus, loading: false });
    } catch (e) {
      const message = messageOf(e);
      set({ error: message, loading: false });
      toast.error(message);
    }
  },
}));
