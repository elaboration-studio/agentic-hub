import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  scan: vi.fn(),
  usageTracingStatus: vi.fn(),
  queryUsageDashboard: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));
vi.mock("./manager", () => ({
  useManagerStore: {
    getState: vi.fn(() => ({
      data: { settings: { sources: [{ id: "default", path: "/tmp", label: "Default" }] } },
    })),
  },
}));

import { queryUsageDashboard, scan, usageTracingStatus } from "@/ipc";
import { toast } from "sonner";
import type { UsageDashboard, UsageTracingStatus } from "@/types";
import { DEFAULT_USAGE_RANGE, useStatisticsStore } from "./statistics";

const mocked = {
  scan: vi.mocked(scan),
  usageTracingStatus: vi.mocked(usageTracingStatus),
  queryUsageDashboard: vi.mocked(queryUsageDashboard),
};

function tracingStatus(enabled: boolean): UsageTracingStatus {
  return {
    enabled,
    collectorRunning: enabled,
    collectorPort: 17321,
    dbPath: "/tmp/trace.db",
    supportedTools: ["cursor"],
    storedEventCount: 10,
    resolvedEventCount: 8,
    unresolvedEventCount: 2,
  };
}

function dashboard(): UsageDashboard {
  return {
    overview: {
      totalEvents: 10,
      terminalEvents: 8,
      resolvedEvents: 8,
      unresolvedEvents: 2,
      tracedCapabilities: 3,
      installedCountable: 12,
      unusedCountable: 9,
    },
    byKind: [{ kind: "skill", executionCount: 6 }],
    bySourceTool: [{ sourceTool: "cursor", executionCount: 6 }],
    byDay: [{ day: "2026-07-07", executionCount: 6 }],
    topCapabilities: [],
    unusedCapabilities: [],
    byWorkspace: [],
  };
}

function getInitialState() {
  return {
    dashboard: null,
    range: DEFAULT_USAGE_RANGE,
    tracingStatus: null,
    loading: false,
    error: null,
    setRange: useStatisticsStore.getState().setRange,
    reload: useStatisticsStore.getState().reload,
  };
}

describe("useStatisticsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStatisticsStore.setState(getInitialState(), true);
    mocked.scan.mockResolvedValue({ items: [], errors: [] });
  });

  it("loads dashboard when tracing is enabled", async () => {
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(true));
    mocked.queryUsageDashboard.mockResolvedValue(dashboard());

    await useStatisticsStore.getState().reload();

    expect(mocked.queryUsageDashboard).toHaveBeenCalledWith([], DEFAULT_USAGE_RANGE);
    expect(useStatisticsStore.getState().dashboard?.overview.tracedCapabilities).toBe(3);
    expect(useStatisticsStore.getState().tracingStatus?.enabled).toBe(true);
  });

  it("skips dashboard query when tracing is disabled", async () => {
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(false));

    await useStatisticsStore.getState().reload();

    expect(mocked.queryUsageDashboard).not.toHaveBeenCalled();
    expect(useStatisticsStore.getState().dashboard).toBeNull();
  });

  it("setRange reloads with the new window", async () => {
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(true));
    mocked.queryUsageDashboard.mockResolvedValue(dashboard());

    useStatisticsStore.getState().setRange("last7Days");
    await vi.waitFor(() => expect(mocked.queryUsageDashboard).toHaveBeenCalled());

    expect(mocked.queryUsageDashboard).toHaveBeenLastCalledWith([], "last7Days");
    expect(useStatisticsStore.getState().range).toBe("last7Days");
  });

  it("surfaces IPC failures as toast errors", async () => {
    mocked.usageTracingStatus.mockRejectedValue(new Error("collector offline"));

    await useStatisticsStore.getState().reload();

    expect(useStatisticsStore.getState().error).toBe("collector offline");
    expect(toast.error).toHaveBeenCalledWith("collector offline");
  });
});
