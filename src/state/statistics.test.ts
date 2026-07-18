import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  scan: vi.fn(),
  loadSettings: vi.fn(),
  usageTracingStatus: vi.fn(),
  queryUsageDashboard: vi.fn(),
  listSkillFavorites: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

const managerGetState = vi.fn();
const managerRefresh = vi.fn();

vi.mock("./manager", () => ({
  useManagerStore: {
    getState: () => managerGetState(),
  },
}));

import { listSkillFavorites, loadSettings, queryUsageDashboard, scan, usageTracingStatus } from "@/ipc";
import { toast } from "sonner";
import type {
  CapabilityItem,
  Settings,
  SkillFavorite,
  ToolId,
  ToolSettings,
  UsageDashboard,
  UsageTracingStatus,
} from "@/types";
import {
  computeResourceInventory,
  DEFAULT_USAGE_RANGE,
  useStatisticsStore,
} from "./statistics";

const mocked = {
  scan: vi.mocked(scan),
  loadSettings: vi.mocked(loadSettings),
  usageTracingStatus: vi.mocked(usageTracingStatus),
  queryUsageDashboard: vi.mocked(queryUsageDashboard),
  listSkillFavorites: vi.mocked(listSkillFavorites),
};

function toolSettings(enabled = true): ToolSettings {
  return {
    enabled,
    skillsPath: "/skills",
    agentsPath: "/agents",
    rulesPath: "/rules",
    instructionsPath: null,
    hooksEnabled: false,
    hooksFile: null,
    hooksDir: null,
    commandsPath: enabled ? "/commands" : null,
  };
}

function makeSettings(overrides: Partial<Record<ToolId, boolean>> = {}): Settings {
  return {
    sources: [
      { id: "default", label: "Default", path: "/shared" },
      { id: "extra", label: "Extra", path: "/extra" },
    ],
    sharedRoot: "/shared",
    suitesPath: null,
    cliToolsPath: null,
    watcherEnabled: true,
    watcherForceMigrated: true,
    codexAgentsPathMigrated: true,
    antigravitySkillsPathMigrated: true,
    kiroRulesAgentsMdMigrated: true,
    editor: { kind: "default", customApp: null },
    colorScheme: "system",
    paletteShortcut: "Cmd+Alt+A",
    pasteIntoFocused: false,
    skills: { enabled: false, favoritesPath: null },
    usageTracing: {
      enabled: false,
      captureTools: ["codex", "claude", "cursor"],
      retentionDays: 90,
      collectorPort: 17321,
      collectorToken: "",
    },
    telemetry: { enabled: false },
    mainWindow: null,
    tools: {
      codex: toolSettings(overrides.codex ?? true),
      claude: toolSettings(overrides.claude ?? false),
      cursor: toolSettings(overrides.cursor ?? true),
      openclaw: toolSettings(overrides.openclaw ?? false),
      openstandard: toolSettings(overrides.openstandard ?? false),
      kiro: toolSettings(overrides.kiro ?? false),
      copilot: toolSettings(overrides.copilot ?? false),
      antigravity: toolSettings(overrides.antigravity ?? false),
    },
  };
}

function makeItem(id: string, kind: CapabilityItem["kind"] = "skill"): CapabilityItem {
  return {
    id,
    kind,
    name: id,
    sourcePath: `/shared/${kind}s/${id}`,
    relativePath: id,
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function favorite(id: string): SkillFavorite {
  return {
    provider: "skills.sh",
    id,
    slug: id,
    name: id,
    source: "owner/repo",
    installRef: "owner/repo",
    githubUrl: null,
    pageUrl: null,
    starredAt: "2026-07-01T00:00:00Z",
  };
}

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
    todayTopCapabilities: [],
    unusedCapabilities: [],
    byWorkspace: [],
  };
}

function seedManager(items: CapabilityItem[], settings = makeSettings()) {
  managerGetState.mockReturnValue({
    data: { items, settings, scanErrors: [], result: { states: [], adapterStatuses: [] } },
    refresh: managerRefresh,
  });
}

function getInitialState() {
  return {
    inventory: null,
    dashboard: null,
    range: DEFAULT_USAGE_RANGE,
    tracingStatus: null,
    loading: false,
    error: null,
    setRange: useStatisticsStore.getState().setRange,
    reload: useStatisticsStore.getState().reload,
  };
}

describe("computeResourceInventory", () => {
  it("groups all five kinds and enabled tools", () => {
    const items = [
      makeItem("skill:a", "skill"),
      makeItem("skill:b", "skill"),
      makeItem("agent:a", "agent"),
      makeItem("rule:a", "rule"),
      makeItem("hook:a", "hook"),
      makeItem("command:a", "command"),
    ];
    const settings = makeSettings({ codex: true, cursor: true, claude: true });

    const inventory = computeResourceInventory(items, settings, [favorite("one"), favorite("two")]);

    expect(inventory.total).toBe(6);
    expect(inventory.byKind.skill).toBe(2);
    expect(inventory.byKind.agent).toBe(1);
    expect(inventory.byKind.rule).toBe(1);
    expect(inventory.byKind.hook).toBe(1);
    expect(inventory.byKind.command).toBe(1);
    expect(
      inventory.byKind.skill +
        inventory.byKind.agent +
        inventory.byKind.rule +
        inventory.byKind.hook +
        inventory.byKind.command,
    ).toBe(inventory.total);
    expect(inventory.sourceCount).toBe(2);
    expect(inventory.enabledTools).toBe(3);
    expect(inventory.totalTools).toBe(8);
    expect(inventory.favoritesCount).toBe(2);
  });
});

describe("useStatisticsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStatisticsStore.setState(getInitialState(), true);
    managerRefresh.mockResolvedValue(undefined);
    mocked.listSkillFavorites.mockResolvedValue({ favorites: [] });
    mocked.scan.mockResolvedValue({ items: [], errors: [] });
    mocked.loadSettings.mockResolvedValue(makeSettings());
    seedManager([]);
  });

  it("loads dashboard when tracing is enabled", async () => {
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(true));
    mocked.queryUsageDashboard.mockResolvedValue(dashboard());

    await useStatisticsStore.getState().reload();

    expect(mocked.queryUsageDashboard).toHaveBeenCalledWith([], DEFAULT_USAGE_RANGE);
    expect(useStatisticsStore.getState().dashboard?.overview.tracedCapabilities).toBe(3);
    expect(useStatisticsStore.getState().tracingStatus?.enabled).toBe(true);
  });

  it("always sets inventory even when tracing is disabled", async () => {
    const items = [makeItem("skill:a", "skill"), makeItem("rule:a", "rule")];
    seedManager(items);
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(false));
    mocked.listSkillFavorites.mockResolvedValue({ favorites: [favorite("starred")] });

    await useStatisticsStore.getState().reload();

    expect(mocked.queryUsageDashboard).not.toHaveBeenCalled();
    expect(useStatisticsStore.getState().dashboard).toBeNull();
    expect(useStatisticsStore.getState().inventory).toEqual({
      total: 2,
      sourceCount: 2,
      enabledTools: 2,
      totalTools: 8,
      favoritesCount: 1,
      byKind: { skill: 1, agent: 0, rule: 1, hook: 0, command: 0 },
    });
  });

  it("uses manager store items without scanning when data is present", async () => {
    const items = [makeItem("skill:a", "skill")];
    seedManager(items);
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(true));
    mocked.queryUsageDashboard.mockResolvedValue(dashboard());

    await useStatisticsStore.getState().reload();

    expect(mocked.scan).not.toHaveBeenCalled();
    expect(mocked.loadSettings).not.toHaveBeenCalled();
    expect(mocked.queryUsageDashboard).toHaveBeenCalledWith(items, DEFAULT_USAGE_RANGE);
    expect(useStatisticsStore.getState().inventory?.total).toBe(1);
  });

  it("refreshes manager when data is missing", async () => {
    const items = [makeItem("skill:a", "skill")];
    managerGetState
      .mockReturnValueOnce({ data: null, refresh: managerRefresh })
      .mockReturnValueOnce({
        data: {
          items,
          settings: makeSettings(),
          scanErrors: [],
          result: { states: [], adapterStatuses: [] },
        },
        refresh: managerRefresh,
      });
    mocked.usageTracingStatus.mockResolvedValue(tracingStatus(false));

    await useStatisticsStore.getState().reload();

    expect(managerRefresh).toHaveBeenCalled();
    expect(useStatisticsStore.getState().inventory?.total).toBe(1);
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
