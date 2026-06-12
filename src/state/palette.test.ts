import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  loadSettings: vi.fn(),
  scan: vi.fn(),
  listSuites: vi.fn(),
  listWorkspaceTargets: vi.fn(),
  scanWorkspace: vi.fn(),
  applySuite: vi.fn().mockResolvedValue(undefined),
  emitHubNavigate: vi.fn().mockResolvedValue(undefined),
  emitHubLocate: vi.fn().mockResolvedValue(undefined),
  setWatcherEnabled: vi.fn().mockResolvedValue(undefined),
  emitHubWatcherChanged: vi.fn().mockResolvedValue(undefined),
  showMain: vi.fn().mockResolvedValue(undefined),
}));

import {
  applySuite,
  listSuites,
  listWorkspaceTargets,
  loadSettings,
  scan,
  scanWorkspace,
} from "@/ipc";
import type {
  CapabilityItem,
  Settings,
  SuiteDefinition,
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
  applySuite: vi.mocked(applySuite),
};

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
    watcherEnabled: true,
    watcherForceMigrated: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    skills: { enabled: false, favoritesPath: null },
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
        : Promise.resolve({ items: [makeItem("skill:qa")], states: [], errors: [] }),
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
    expect(mocked.applySuite).not.toHaveBeenCalled();
  });

  it("running a tool row applies the suite to that one tool", async () => {
    await loadReady();
    usePaletteStore.getState().enterSuite("s1", "Backend");
    const cursorIdx = usePaletteStore
      .getState()
      .results.findIndex((r) => r.title === "Apply to Cursor");

    usePaletteStore.getState().setSelected(cursorIdx);
    await usePaletteStore.getState().runSelected();

    expect(mocked.applySuite).toHaveBeenCalledWith("cursor", "s1");
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
