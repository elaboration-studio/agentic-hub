import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  openPath: vi.fn(),
  applySuite: vi.fn().mockResolvedValue(undefined),
}));

import { applySuite, openPath } from "@/ipc";
import type { CapabilityItem, Settings, SuiteDefinition, ToolId, ToolSettings } from "@/types";
import { computeResults, computeSuiteToolResults, type ProviderContext } from "./commands";

const mocked = {
  openPath: vi.mocked(openPath),
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

function makeSettings(overrides: Partial<Record<ToolId, boolean>> = {}): Settings {
  return {
    sources: [],
    sharedRoot: "/shared",
    suitesPath: null,
    watcherEnabled: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    tools: {
      codex: toolSettings(overrides.codex ?? true),
      claude: toolSettings(overrides.claude ?? true),
      cursor: toolSettings(overrides.cursor ?? true),
      openclaw: toolSettings(overrides.openclaw ?? false),
    },
  };
}

function makeSuite(overrides: Partial<SuiteDefinition> = {}): SuiteDefinition {
  return {
    id: "s1",
    name: "Backend",
    description: null,
    capabilities: [
      { cap: "skill:a", source: null },
      { cap: "skill:b", source: null },
    ],
    createdAt: "t",
    updatedAt: "t",
    ...overrides,
  };
}

function makeItem(id: string, name: string): CapabilityItem {
  return {
    id,
    kind: "skill",
    name,
    sourcePath: `/shared/skills/${name}`,
    relativePath: name,
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function ctx(overrides: Partial<ProviderContext> = {}): ProviderContext {
  return {
    settings: makeSettings(),
    items: [],
    suites: [],
    query: "",
    navigate: vi.fn(),
    enterSuite: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => vi.clearAllMocks());

describe("root providers", () => {
  it("suite provider lists every suite on an empty query, filters by name otherwise", () => {
    const suites = [makeSuite({ id: "a", name: "Backend" }), makeSuite({ id: "b", name: "Frontend" })];

    const all = computeResults(ctx({ suites }));
    expect(all.filter((r) => r.group === "Suite").map((r) => r.title)).toEqual(["Backend", "Frontend"]);

    const filtered = computeResults(ctx({ suites, query: "front" }));
    expect(filtered.filter((r) => r.group === "Suite").map((r) => r.title)).toEqual(["Frontend"]);
  });

  it("a suite row drills in (does not dismiss or execute) and calls enterSuite", () => {
    const enterSuite = vi.fn();
    const results = computeResults(ctx({ suites: [makeSuite({ id: "s1", name: "Backend" })], enterSuite }));
    const suiteRow = results.find((r) => r.group === "Suite")!;

    expect(suiteRow.dismissOnRun).toBe(false);
    suiteRow.run();
    expect(enterSuite).toHaveBeenCalledWith("s1", "Backend");
  });

  it("resource search opens the original file and is terminal", () => {
    const items = [makeItem("skill:tdd", "tdd")];
    const results = computeResults(ctx({ items, query: "tdd" }));
    const row = results.find((r) => r.id === "resource:skill:tdd")!;

    expect(row.dismissOnRun).toBeUndefined();
    row.run();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/skills/tdd/SKILL.md", undefined);
  });
});

describe("suite-tools view", () => {
  it("lists one apply row per enabled tool (openclaw disabled is excluded)", () => {
    const rows = computeSuiteToolResults(makeSettings(), "", "s1", "Backend");
    expect(rows.map((r) => r.title)).toEqual(["Apply to Codex", "Apply to Claude", "Apply to Cursor"]);
    expect(rows.every((r) => r.group === "Apply")).toBe(true);
  });

  it("filters tool rows by the query", () => {
    const rows = computeSuiteToolResults(makeSettings(), "claude", "s1", "Backend");
    expect(rows.map((r) => r.title)).toEqual(["Apply to Claude"]);
  });

  it("running an apply row applies the suite to that one tool (full reset)", () => {
    const rows = computeSuiteToolResults(makeSettings(), "", "s1", "Backend");
    rows.find((r) => r.title === "Apply to Cursor")!.run();
    expect(mocked.applySuite).toHaveBeenCalledWith("cursor", "s1");
  });
});
