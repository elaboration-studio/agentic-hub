import { beforeEach, describe, expect, it, vi } from "vitest";

const { requestApply } = vi.hoisted(() => ({
  requestApply: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/state/apply", () => ({
  useApplyStore: { getState: () => ({ request: requestApply }) },
}));

vi.mock("@/ipc", () => ({
  loadSettings: vi.fn(),
  scan: vi.fn(),
  listSuites: vi.fn(),
  listWorkspaceTargets: vi.fn(),
  scanWorkspace: vi.fn(),
  emitHubNavigate: vi.fn().mockResolvedValue(undefined),
  emitHubLocate: vi.fn().mockResolvedValue(undefined),
  setWatcherEnabled: vi.fn().mockResolvedValue(undefined),
  emitHubWatcherChanged: vi.fn().mockResolvedValue(undefined),
  showMain: vi.fn().mockResolvedValue(undefined),
  inspect: vi.fn().mockResolvedValue({ states: [], adapterStatuses: [] }),
  suiteOwnership: vi.fn().mockResolvedValue([]),
  plan: vi.fn().mockResolvedValue([]),
  apply: vi.fn().mockResolvedValue({ created: 0, removed: 0, replaced: 0, errors: [] }),
  syncRules: vi.fn().mockResolvedValue({}),
  syncHooks: vi.fn().mockResolvedValue({}),
  emitSourcesChanged: vi.fn().mockResolvedValue(undefined),
}));

import {
  apply,
  emitSourcesChanged,
  inspect,
  listSuites,
  listWorkspaceTargets,
  loadSettings,
  plan,
  scan,
  scanWorkspace,
  suiteOwnership,
  syncHooks,
  syncRules,
} from "@/ipc";
import type {
  CapabilityItem,
  Settings,
  SuiteDefinition,
  ToolCapabilityState,
  ToolId,
  ToolSettings,
  WorkspaceTarget,
} from "@/types";
import { usePaletteStore } from "./palette";

const mocked = {
  loadSettings: vi.mocked(loadSettings),
  scan: vi.mocked(scan),
  listSuites: vi.mocked(listSuites),
  listWorkspaceTargets: vi.mocked(listWorkspaceTargets),
  scanWorkspace: vi.mocked(scanWorkspace),
  requestApply,
  inspect: vi.mocked(inspect),
  suiteOwnership: vi.mocked(suiteOwnership),
  plan: vi.mocked(plan),
  apply: vi.mocked(apply),
  syncRules: vi.mocked(syncRules),
  syncHooks: vi.mocked(syncHooks),
  emitSourcesChanged: vi.mocked(emitSourcesChanged),
};

// Flush queued microtasks so the background inspect kicked off by load()/drill-in
// has resolved before assertions.
const flush = () => new Promise((r) => setTimeout(r, 0));

function inspectState(tool: ToolId, itemId: string, on: boolean): ToolCapabilityState {
  return {
    tool,
    itemId,
    targetPath: `/${tool}/${itemId}`,
    state: on ? "enabled" : "disabled",
    currentLinkTarget: on ? `/shared/${itemId}` : null,
  };
}

function makeKindItem(id: string, kind: CapabilityItem["kind"]): CapabilityItem {
  return {
    id,
    kind,
    name: id.slice(id.indexOf(":") + 1),
    sourcePath: `/shared/${kind}s/${id}`,
    relativePath: id.slice(id.indexOf(":") + 1),
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function makeTarget(id: string, label: string): WorkspaceTarget {
  return { id, label, dir: `/repos/${label}`, lastUsedAt: "t" };
}

function toolSettings(enabled: boolean): ToolSettings {
  return {
    enabled,
    skillsPath: "/skills",
    agentsPath: "/agents",
    rulesPath: "/rules",
    instructionsPath: null,
    hooksEnabled: false,
    hooksFile: null,
    commandsPath: enabled ? "/commands" : null,
  };
}

function makeSettings(): Settings {
  return {
    sources: [],
    sharedRoot: "/shared",
    suitesPath: null,
    cliToolsPath: null,
    watcherEnabled: true,
    watcherForceMigrated: true,
    codexAgentsPathMigrated: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    skills: { enabled: false, favoritesPath: null },
    telemetry: { enabled: false },
    tools: {
      codex: toolSettings(true),
      claude: toolSettings(true),
      cursor: toolSettings(true),
      openclaw: toolSettings(false),
      openstandard: toolSettings(false),
    },
  };
}

function makeSuite(id: string, name: string): SuiteDefinition {
  return {
    id,
    name,
    description: null,
    capabilities: [{ cap: "skill:a", source: null }],
    isBase: false,
    createdAt: "t",
    updatedAt: "t",
  };
}

function makeItem(id: string): CapabilityItem {
  return {
    id,
    kind: "skill",
    name: id.replace("skill:", ""),
    sourcePath: `/shared/skills/${id}`,
    relativePath: id.replace("skill:", ""),
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

async function loadReady(suites: SuiteDefinition[] = [makeSuite("s1", "Backend")]) {
  mocked.loadSettings.mockResolvedValue(makeSettings());
  mocked.scan.mockResolvedValue({ items: [makeItem("skill:tdd")], errors: [] });
  mocked.listSuites.mockResolvedValue(suites);
  mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
  await usePaletteStore.getState().load();
}

beforeEach(() => {
  vi.clearAllMocks();
  usePaletteStore.setState(usePaletteStore.getInitialState(), true);
  // Re-establish default resolutions (clearAllMocks wipes call history; keep the
  // pipeline mocks benign unless a test overrides them).
  mocked.inspect.mockResolvedValue({ states: [], adapterStatuses: [] });
  mocked.suiteOwnership.mockResolvedValue([]);
  mocked.plan.mockResolvedValue([]);
  mocked.apply.mockResolvedValue({ created: 0, removed: 0, replaced: 0, refreshed: 0, skipped: 0, errors: [] });
  mocked.syncRules.mockResolvedValue({ outcome: "unchanged", errors: [] } as never);
  mocked.syncHooks.mockResolvedValue({ outcome: "unchanged", notes: [], errors: [] } as never);
  mocked.emitSourcesChanged.mockResolvedValue(undefined);
});

describe("palette store — loading", () => {
  it("load fetches settings, scan, and suites and becomes ready at the root hub", async () => {
    await loadReady();
    const s = usePaletteStore.getState();

    expect(s.status).toBe("ready");
    expect(s.view).toEqual({ kind: "root" });
    expect(s.suites).toHaveLength(1);
    // The root is the sectioned hub — drill-in modes, no resource or suite rows.
    expect(s.results.some((r) => r.title === "Search all resources")).toBe(true);
    expect(s.results.some((r) => r.group === "Suite")).toBe(false);
    expect(s.results.some((r) => r.id.startsWith("resource:"))).toBe(false);
  });

  it("load surfaces an error when IPC throws", async () => {
    mocked.loadSettings.mockRejectedValue(new Error("disk gone"));
    await usePaletteStore.getState().load();
    expect(usePaletteStore.getState().status).toBe("error");
    expect(usePaletteStore.getState().error).toBe("disk gone");
  });
});

describe("palette store — workspace inventories", () => {
  it("load scans every remembered workspace and populates results on a query", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("w1", "alpha"), makeTarget("w2", "beta")],
      workspaceActiveId: "w1",
    });
    mocked.scanWorkspace.mockImplementation((id: string) =>
      Promise.resolve({
        items: [makeItem(`skill:qa-${id}`)],
        states: [],
        errors: [],
        lockedSkills: [],
      }),
    );

    await usePaletteStore.getState().load();
    expect(usePaletteStore.getState().workspaces).toHaveLength(2);

    // Workspace results live behind the "Go to workspace" mode, not the root.
    usePaletteStore.getState().setQuery("qa");
    expect(usePaletteStore.getState().results).toEqual([]);

    usePaletteStore.getState().enterMode("workspace");
    usePaletteStore.getState().setQuery("qa");
    const rows = usePaletteStore.getState().results.filter((r) => r.group === "Workspace");
    expect(rows.map((r) => r.subtitle)).toEqual(["alpha · qa-w1", "beta · qa-w2"]);
  });

  it("skips a workspace whose scan fails without breaking summon", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("w1", "alpha"), makeTarget("w2", "beta")],
      workspaceActiveId: "w1",
    });
    mocked.scanWorkspace.mockImplementation((id: string) =>
      id === "w2"
        ? Promise.reject(new Error("unreadable"))
        : Promise.resolve({
            items: [makeItem("skill:qa")],
            states: [],
            errors: [],
            lockedSkills: [],
          }),
    );

    await usePaletteStore.getState().load();
    const s = usePaletteStore.getState();
    expect(s.status).toBe("ready");
    expect(s.workspaces.map((w) => w.target.id)).toEqual(["w1"]);
  });
});

describe("palette store — layered navigation", () => {
  it("running a hub mode row enters that search view with a clean query", async () => {
    await loadReady();
    usePaletteStore.getState().setQuery("skills");
    const idx = usePaletteStore
      .getState()
      .results.findIndex((r) => r.title === "Search skills");
    expect(idx).toBeGreaterThanOrEqual(0);

    usePaletteStore.getState().setSelected(idx);
    await usePaletteStore.getState().runSelected();

    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "search", mode: "skill" });
    expect(s.query).toBe("");
    // Empty query inside a search mode shows nothing until the user types.
    expect(s.results).toEqual([]);
  });

  it("enterMode switches between search modes and clears the query", async () => {
    await loadReady();
    usePaletteStore.getState().enterMode("skill");
    usePaletteStore.getState().setQuery("foo");

    usePaletteStore.getState().enterMode("agent");
    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "search", mode: "agent" });
    expect(s.query).toBe("");
    expect(s.results).toEqual([]);
  });

  it("back pops a search mode to the root hub", async () => {
    await loadReady();
    usePaletteStore.getState().enterMode("skill");

    usePaletteStore.getState().back();
    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "root" });
    expect(s.results.some((r) => r.title === "Search all resources")).toBe(true);
  });
});

describe("palette store — suite flow (suite mode → suite-tools)", () => {
  it("running a suite row in the suite mode enters suite-tools without executing", async () => {
    await loadReady();
    usePaletteStore.getState().enterMode("suite");
    const suiteRow = usePaletteStore.getState().results.find((r) => r.group === "Suite");
    expect(suiteRow).toBeDefined();

    // Selecting and running the suite row drills in.
    const idx = usePaletteStore.getState().results.indexOf(suiteRow!);
    usePaletteStore.getState().setSelected(idx);
    await usePaletteStore.getState().runSelected();

    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "suite-tools", suiteId: "s1", suiteName: "Backend" });
    expect(s.query).toBe("");
    // Tool apply rows now populate the list (openclaw disabled).
    expect(s.results.map((r) => r.title)).toEqual([
      "Apply to Codex",
      "Apply to Claude",
      "Apply to Cursor",
    ]);
    expect(mocked.requestApply).not.toHaveBeenCalled();
  });

  it("running a tool row routes through the apply store preview flow", async () => {
    await loadReady();
    usePaletteStore.getState().enterSuite("s1", "Backend");
    const cursorIdx = usePaletteStore
      .getState()
      .results.findIndex((r) => r.title === "Apply to Cursor");

    usePaletteStore.getState().setSelected(cursorIdx);
    await usePaletteStore.getState().runSelected();

    expect(mocked.requestApply).toHaveBeenCalledWith("cursor", "s1", "Backend");
  });

  it("back from suite-tools returns to the suite search mode, then the root", async () => {
    await loadReady();
    usePaletteStore.getState().enterMode("suite");
    usePaletteStore.getState().enterSuite("s1", "Backend");
    expect(usePaletteStore.getState().view.kind).toBe("suite-tools");

    usePaletteStore.getState().back();
    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "search", mode: "suite" });
    expect(s.results.some((r) => r.group === "Suite")).toBe(true);

    usePaletteStore.getState().back();
    expect(usePaletteStore.getState().view).toEqual({ kind: "root" });
  });

  it("reset returns straight to the root view and clears the query", async () => {
    await loadReady();
    usePaletteStore.getState().enterSuite("s1", "Backend");
    usePaletteStore.getState().setQuery("claude");

    usePaletteStore.getState().reset();
    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "root" });
    expect(s.query).toBe("");
  });
});

describe("palette store — capability-tools (inline per-tool toggle)", () => {
  it("inspects per-tool state in the background and populates currentMap", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [makeKindItem("skill:tdd", "skill")], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.inspect.mockResolvedValue({
      states: [inspectState("claude", "skill:tdd", true)],
      adapterStatuses: [],
    });

    await usePaletteStore.getState().load();
    await flush();

    const s = usePaletteStore.getState();
    expect(s.inspected).toBe(true);
    expect(s.currentMap.get("claude::skill:tdd")?.state).toBe("enabled");
  });

  it("drilling into a resource shows per-tool toggle rows reflecting state", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [makeKindItem("skill:tdd", "skill")], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.inspect.mockResolvedValue({
      states: [
        inspectState("codex", "skill:tdd", false),
        inspectState("claude", "skill:tdd", true),
        inspectState("cursor", "skill:tdd", false),
      ],
      adapterStatuses: [],
    });

    await usePaletteStore.getState().load();
    await flush();

    usePaletteStore.getState().enterMode("skill");
    usePaletteStore.getState().setQuery("tdd");
    const row = usePaletteStore.getState().results.find((r) => r.id === "resource:skill:tdd")!;
    expect(row.dismissOnRun).toBe(false);
    await row.run();

    const s = usePaletteStore.getState();
    expect(s.view).toMatchObject({ kind: "capability-tools", itemId: "skill:tdd", fromMode: "skill" });
    const toolRows = s.results.filter((r) => r.group === "Tool" && !r.id.startsWith("captool-all"));
    expect(toolRows.map((r) => [r.title, r.state])).toEqual([
      ["Codex", "off"],
      ["Claude", "on"],
      ["Cursor", "off"],
    ]);
  });

  it("toggleCapability passes the COMPLETE desired map so other rules survive", async () => {
    // The footgun: syncRules/syncHooks rewrite the whole managed block, so a
    // single toggle must carry every other enabled item as still-enabled.
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({
      items: [
        makeKindItem("rule:a", "rule"),
        makeKindItem("rule:b", "rule"),
        makeKindItem("rule:c", "rule"),
      ],
      errors: [],
    });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.inspect.mockResolvedValue({
      states: [
        inspectState("claude", "rule:a", true),
        inspectState("claude", "rule:b", true),
        inspectState("claude", "rule:c", false),
      ],
      adapterStatuses: [],
    });

    await usePaletteStore.getState().load();
    await flush();
    await usePaletteStore.getState().toggleCapability("claude", "rule:c");

    // Every sync carries a:true, b:true (preserved) and c:true (the flip).
    const desired = { "rule:a": true, "rule:b": true, "rule:c": true };
    expect(mocked.syncRules).toHaveBeenCalledWith("claude", expect.any(Array), desired);
    expect(mocked.syncHooks).toHaveBeenCalledWith("claude", expect.any(Array), desired);
    expect(mocked.emitSourcesChanged).toHaveBeenCalled();
  });

  it("toggleCapability applies the plan ops and re-inspects to reconcile", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [makeKindItem("skill:tdd", "skill")], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.inspect.mockResolvedValue({
      states: [inspectState("claude", "skill:tdd", false)],
      adapterStatuses: [],
    });
    mocked.plan.mockResolvedValue([{ kind: "create" } as never]);

    await usePaletteStore.getState().load();
    await flush();
    mocked.inspect.mockClear();
    await usePaletteStore.getState().toggleCapability("claude", "skill:tdd");

    expect(mocked.plan).toHaveBeenCalledWith("claude", expect.any(Array), { "skill:tdd": true });
    expect(mocked.apply).toHaveBeenCalled();
    // Reconcile re-inspects so the panel shows what actually landed.
    expect(mocked.inspect).toHaveBeenCalled();
  });

  it("toggleCapability is a no-op on a suite-locked cell", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [makeKindItem("skill:tdd", "skill")], errors: [] });
    mocked.listSuites.mockResolvedValue([]);
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.inspect.mockResolvedValue({
      states: [inspectState("claude", "skill:tdd", true)],
      adapterStatuses: [],
    });
    mocked.suiteOwnership.mockResolvedValue([
      { tool: "claude", itemId: "skill:tdd", suiteId: "s1", suiteName: "Backend", fromBase: false },
    ]);

    await usePaletteStore.getState().load();
    await flush();
    await usePaletteStore.getState().toggleCapability("claude", "skill:tdd");

    expect(mocked.plan).not.toHaveBeenCalled();
    expect(mocked.syncRules).not.toHaveBeenCalled();
  });

  it("back from capability-tools returns to the originating search mode", async () => {
    await loadReady([]);
    await flush();
    usePaletteStore.getState().enterCapabilityTools(makeKindItem("agent:bot", "agent"), "agent");
    expect(usePaletteStore.getState().view.kind).toBe("capability-tools");

    usePaletteStore.getState().back();
    expect(usePaletteStore.getState().view).toEqual({ kind: "search", mode: "agent" });
  });
});

describe("palette store — selection", () => {
  it("move wraps around both ends", async () => {
    await loadReady();
    const len = usePaletteStore.getState().results.length;
    expect(len).toBeGreaterThan(0);

    usePaletteStore.getState().setSelected(0);
    usePaletteStore.getState().move(-1);
    expect(usePaletteStore.getState().selectedIndex).toBe(len - 1);
    usePaletteStore.getState().move(1);
    expect(usePaletteStore.getState().selectedIndex).toBe(0);
  });
});
