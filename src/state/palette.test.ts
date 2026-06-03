import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  loadSettings: vi.fn(),
  scan: vi.fn(),
  listSuites: vi.fn(),
  applySuite: vi.fn().mockResolvedValue(undefined),
  emitHubNavigate: vi.fn().mockResolvedValue(undefined),
  showMain: vi.fn().mockResolvedValue(undefined),
}));

import { applySuite, listSuites, loadSettings, scan } from "@/ipc";
import type { CapabilityItem, Settings, SuiteDefinition, ToolSettings } from "@/types";
import { usePaletteStore } from "./palette";

const mocked = {
  loadSettings: vi.mocked(loadSettings),
  scan: vi.mocked(scan),
  listSuites: vi.mocked(listSuites),
  applySuite: vi.mocked(applySuite),
};

function toolSettings(enabled: boolean): ToolSettings {
  return {
    enabled,
    skillsPath: "/skills",
    agentsPath: "/agents",
    rulesPath: "/rules",
    instructionsPath: null,
    hooksEnabled: false,
    hooksFile: null,
  };
}

function makeSettings(): Settings {
  return {
    sources: [],
    sharedRoot: "/shared",
    suitesPath: null,
    watcherEnabled: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    tools: {
      codex: toolSettings(true),
      claude: toolSettings(true),
      cursor: toolSettings(true),
      openclaw: toolSettings(false),
    },
  };
}

function makeSuite(id: string, name: string): SuiteDefinition {
  return {
    id,
    name,
    description: null,
    capabilities: ["skill:a"],
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
    valid: true,
    validationErrors: [],
  };
}

async function loadReady(suites: SuiteDefinition[] = [makeSuite("s1", "Backend")]) {
  mocked.loadSettings.mockResolvedValue(makeSettings());
  mocked.scan.mockResolvedValue({ items: [makeItem("skill:tdd")], errors: [] });
  mocked.listSuites.mockResolvedValue(suites);
  await usePaletteStore.getState().load();
}

beforeEach(() => {
  vi.clearAllMocks();
  usePaletteStore.setState(usePaletteStore.getInitialState(), true);
});

describe("palette store — loading", () => {
  it("load fetches settings, scan, and suites and becomes ready at the root view", async () => {
    await loadReady();
    const s = usePaletteStore.getState();

    expect(s.status).toBe("ready");
    expect(s.view).toEqual({ kind: "root" });
    expect(s.suites).toHaveLength(1);
    // Suites surface on an empty query.
    expect(s.results.some((r) => r.group === "Suite")).toBe(true);
  });

  it("load surfaces an error when IPC throws", async () => {
    mocked.loadSettings.mockRejectedValue(new Error("disk gone"));
    await usePaletteStore.getState().load();
    expect(usePaletteStore.getState().status).toBe("error");
    expect(usePaletteStore.getState().error).toBe("disk gone");
  });
});

describe("palette store — two-level suite flow", () => {
  it("running a suite row enters the suite-tools view without executing", async () => {
    await loadReady();
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

  it("back returns to the root view and restores root results", async () => {
    await loadReady();
    usePaletteStore.getState().enterSuite("s1", "Backend");
    expect(usePaletteStore.getState().view.kind).toBe("suite-tools");

    usePaletteStore.getState().back();
    const s = usePaletteStore.getState();
    expect(s.view).toEqual({ kind: "root" });
    expect(s.results.some((r) => r.group === "Suite")).toBe(true);
  });

  it("reset returns to the root view and clears the query", async () => {
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
