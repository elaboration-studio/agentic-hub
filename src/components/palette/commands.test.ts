import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  openPath: vi.fn(),
  applySuite: vi.fn().mockResolvedValue(undefined),
  readCapabilityBody: vi.fn().mockResolvedValue("command body text"),
  copyText: vi.fn().mockResolvedValue(undefined),
  setWatcherEnabled: vi.fn().mockResolvedValue(undefined),
  emitHubWatcherChanged: vi.fn().mockResolvedValue(undefined),
}));

import {
  applySuite,
  copyText,
  emitHubWatcherChanged,
  openPath,
  readCapabilityBody,
  setWatcherEnabled,
} from "@/ipc";
import type {
  CapabilityItem,
  Settings,
  SuiteDefinition,
  ToolId,
  ToolSettings,
  WorkspaceTarget,
} from "@/types";
import {
  SEARCH_MODES,
  computeHubResults,
  computeSearchResults,
  computeSuiteToolResults,
  searchModeFromShortcut,
  type ProviderContext,
  type WorkspaceInventoryEntry,
} from "./commands";

const mocked = {
  openPath: vi.mocked(openPath),
  applySuite: vi.mocked(applySuite),
  readCapabilityBody: vi.mocked(readCapabilityBody),
  copyText: vi.mocked(copyText),
  setWatcherEnabled: vi.mocked(setWatcherEnabled),
  emitHubWatcherChanged: vi.mocked(emitHubWatcherChanged),
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
    commandsPath: enabled ? "/commands" : null,
  };
}

function makeSettings(overrides: Partial<Record<ToolId, boolean>> = {}): Settings {
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
      codex: toolSettings(overrides.codex ?? true),
      claude: toolSettings(overrides.claude ?? true),
      cursor: toolSettings(overrides.cursor ?? true),
      openclaw: toolSettings(overrides.openclaw ?? false),
      openstandard: toolSettings(overrides.openstandard ?? false),
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
    isBase: false,
    createdAt: "t",
    updatedAt: "t",
    ...overrides,
  };
}

function makeItem(id: string, name: string, kind: CapabilityItem["kind"] = "skill"): CapabilityItem {
  return {
    id,
    kind,
    name,
    sourcePath: `/shared/${kind}s/${name}`,
    relativePath: name,
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function makeCommand(id: string, name: string, relativePath = name): CapabilityItem {
  return {
    id,
    kind: "command",
    name,
    sourcePath: `/shared/commands/${relativePath}`,
    relativePath,
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function makeTarget(overrides: Partial<WorkspaceTarget> = {}): WorkspaceTarget {
  return { id: "w1", label: "my-project", dir: "/repos/my-project", lastUsedAt: "t", ...overrides };
}

function makeWorkspace(
  target: WorkspaceTarget,
  items: CapabilityItem[],
): WorkspaceInventoryEntry {
  return { target, items };
}

function ctx(overrides: Partial<ProviderContext> = {}): ProviderContext {
  return {
    settings: makeSettings(),
    items: [],
    suites: [],
    workspaces: [],
    query: "",
    navigate: vi.fn(),
    enterMode: vi.fn(),
    enterSuite: vi.fn(),
    locate: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => vi.clearAllMocks());

describe("searchModeFromShortcut", () => {
  it("maps Ctrl+1…7 to the hub search modes in order", () => {
    expect(SEARCH_MODES).toEqual([
      "all",
      "skill",
      "agent",
      "rule",
      "hook",
      "command",
      "suite",
    ]);
    expect(searchModeFromShortcut(1)).toBe("all");
    expect(searchModeFromShortcut(2)).toBe("skill");
    expect(searchModeFromShortcut(7)).toBe("suite");
    expect(searchModeFromShortcut(0)).toBeNull();
    expect(searchModeFromShortcut(8)).toBeNull();
  });
});

describe("root hub", () => {
  it("labels search-mode hub rows with Ctrl+1…7 shortcuts", () => {
    const rows = computeHubResults(ctx());
    const searchRows = rows.filter((r) => r.section === "Search");
    expect(searchRows.map((r) => r.shortcut)).toEqual([
      "⌃1",
      "⌃2",
      "⌃3",
      "⌃4",
      "⌃5",
      "⌃6",
      "⌃7",
    ]);
  });

  it("lists the categorized first-class commands on an empty query — no resources", () => {
    const items = [makeItem("skill:tdd", "tdd"), makeCommand("command:commit", "commit")];
    const rows = computeHubResults(ctx({ items, suites: [makeSuite()] }));

    expect(rows.map((r) => r.title)).toEqual([
      "Search all resources",
      "Search skills",
      "Search agents",
      "Search rules",
      "Search hooks",
      "Search commands",
      "Search suites",
      "Go to global",
      "Go to workspace",
      "Open Manager",
      "Open Suites",
      "Open Config",
      "Apply suite…",
      "Pause watching",
    ]);
    // No resource, suite, or workspace rows leak into the hub.
    expect(rows.every((r) => r.section !== undefined)).toBe(true);
  });

  it("typing at the root filters hub rows only — never surfaces resources", () => {
    const items = [makeItem("skill:tdd", "tdd")];
    const rows = computeHubResults(ctx({ items, query: "tdd" }));
    expect(rows).toEqual([]);

    // "skills" matches the skill mode row and the all-resources subtitle.
    const skillRows = computeHubResults(ctx({ items, query: "skills" }));
    expect(skillRows.map((r) => r.title)).toEqual(["Search all resources", "Search skills"]);
  });

  it("a search-mode row drills in (does not dismiss) and calls enterMode", () => {
    const enterMode = vi.fn();
    const row = computeHubResults(ctx({ enterMode })).find((r) => r.title === "Search skills")!;

    expect(row.dismissOnRun).toBe(false);
    row.run();
    expect(enterMode).toHaveBeenCalledWith("skill");
  });

  it("Apply suite… drills into the suite search mode", () => {
    const enterMode = vi.fn();
    const row = computeHubResults(ctx({ enterMode })).find((r) => r.title === "Apply suite…")!;

    expect(row.dismissOnRun).toBe(false);
    row.run();
    expect(enterMode).toHaveBeenCalledWith("suite");
  });

  it("a navigation row routes the main window", () => {
    const navigate = vi.fn();
    const row = computeHubResults(ctx({ navigate })).find((r) => r.title === "Open Suites")!;

    expect(row.dismissOnRun).toBeUndefined();
    row.run();
    expect(navigate).toHaveBeenCalledWith("suites");
  });

  it("the watching action flips the persisted state and notifies the main window", async () => {
    const row = computeHubResults(ctx()).find((r) => r.id === "action:toggle-watching")!;
    expect(row.title).toBe("Pause watching");

    await row.run();
    expect(mocked.setWatcherEnabled).toHaveBeenCalledWith(false);
    expect(mocked.emitHubWatcherChanged).toHaveBeenCalledWith(false);
  });

  it("the watching action reads the current state for its label and direction", async () => {
    const settings = { ...makeSettings(), watcherEnabled: false };
    const row = computeHubResults(ctx({ settings })).find(
      (r) => r.id === "action:toggle-watching",
    )!;
    expect(row.title).toBe("Resume watching");

    await row.run();
    expect(mocked.setWatcherEnabled).toHaveBeenCalledWith(true);
    expect(mocked.emitHubWatcherChanged).toHaveBeenCalledWith(true);
  });
});

describe("search modes — resources", () => {
  it("a kind mode returns only that kind and opens the original file", () => {
    const items = [
      makeItem("skill:review", "review"),
      makeItem("agent:review-bot", "review-bot", "agent"),
    ];
    const rows = computeSearchResults(ctx({ items, query: "review" }), "skill");

    expect(rows.map((r) => r.id)).toEqual(["resource:skill:review"]);
    rows[0].run();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/skills/review/SKILL.md", undefined);
  });

  it("the all mode searches every non-command kind", () => {
    const items = [
      makeItem("skill:review", "review"),
      makeItem("agent:review-bot", "review-bot", "agent"),
      makeCommand("command:review/code-review.md", "code-review", "review/code-review.md"),
    ];
    const rows = computeSearchResults(ctx({ items, query: "review" }), "all");

    expect(rows.map((r) => r.id)).toEqual(["resource:skill:review", "resource:agent:review-bot"]);
  });

  it("returns nothing on an empty query (does not dump the tree)", () => {
    const items = [makeItem("skill:tdd", "tdd")];
    expect(computeSearchResults(ctx({ items }), "all")).toEqual([]);
    expect(computeSearchResults(ctx({ items }), "skill")).toEqual([]);
  });
});

describe("search modes — commands", () => {
  it("Enter copies the command body to the clipboard", async () => {
    const items = [makeCommand("command:git/commit.md", "commit", "git/commit.md")];
    const row = computeSearchResults(ctx({ items, query: "commit" }), "command")[0];

    await row.run();
    expect(mocked.readCapabilityBody).toHaveBeenCalledWith("/shared/commands/git/commit.md");
    expect(mocked.copyText).toHaveBeenCalledWith("command body text");
    expect(mocked.openPath).not.toHaveBeenCalled();
  });

  it("Alt+Enter opens the source file for editing", async () => {
    const items = [makeCommand("command:git/commit.md", "commit", "git/commit.md")];
    const row = computeSearchResults(ctx({ items, query: "commit" }), "command")[0];

    await row.altRun!();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/commands/git/commit.md", undefined);
    expect(mocked.copyText).not.toHaveBeenCalled();
  });
});

describe("search modes — suites", () => {
  it("lists every suite on an empty query, filters by name otherwise", () => {
    const suites = [makeSuite({ id: "a", name: "Backend" }), makeSuite({ id: "b", name: "Frontend" })];

    const all = computeSearchResults(ctx({ suites }), "suite");
    expect(all.map((r) => r.title)).toEqual(["Backend", "Frontend"]);

    const filtered = computeSearchResults(ctx({ suites, query: "front" }), "suite");
    expect(filtered.map((r) => r.title)).toEqual(["Frontend"]);
  });

  it("a suite row drills in (does not dismiss or execute) and calls enterSuite", () => {
    const enterSuite = vi.fn();
    const rows = computeSearchResults(
      ctx({ suites: [makeSuite({ id: "s1", name: "Backend" })], enterSuite }),
      "suite",
    );

    expect(rows[0].dismissOnRun).toBe(false);
    rows[0].run();
    expect(enterSuite).toHaveBeenCalledWith("s1", "Backend");
  });
});

describe("go-to modes", () => {
  it("global locate matches shared resources and locates without opening", () => {
    const locate = vi.fn();
    const items = [makeItem("skill:qa", "qa"), makeItem("skill:tdd", "tdd")];
    const rows = computeSearchResults(ctx({ items, query: "qa", locate }), "global");

    expect(rows.map((r) => r.id)).toEqual(["global:skill:qa"]);
    rows[0].run();
    expect(locate).toHaveBeenCalledWith({ scope: "global", itemId: "skill:qa" });
    expect(mocked.openPath).not.toHaveBeenCalled();
  });

  it("workspace locate matches across remembered workspaces (including labels)", () => {
    const workspaces = [
      makeWorkspace(makeTarget({ id: "w1", label: "portfolio" }), [
        makeItem("rule:overview", "01-project-overview", "rule"),
        makeItem("rule:i18n", "07-i18n", "rule"),
      ]),
      makeWorkspace(makeTarget({ id: "w2", label: "aicw" }), [
        makeItem("rule:styling", "04-styling", "rule"),
      ]),
    ];
    const rows = computeSearchResults(ctx({ workspaces, query: "portfolio" }), "workspace");

    expect(rows.map((r) => r.title)).toEqual(["01-project-overview", "07-i18n"]);
  });

  it("running a workspace row locates with the scoped payload", () => {
    const locate = vi.fn();
    const workspaces = [makeWorkspace(makeTarget({ id: "w1" }), [makeItem("skill:qa", "qa")])];
    const rows = computeSearchResults(ctx({ workspaces, query: "qa", locate }), "workspace");

    expect(rows[0].dismissOnRun).toBeUndefined();
    rows[0].run();
    expect(locate).toHaveBeenCalledWith({ scope: "workspace", workspaceId: "w1", itemId: "skill:qa" });
  });

  it("returns nothing on an empty query (does not dump every project)", () => {
    const workspaces = [makeWorkspace(makeTarget(), [makeItem("skill:qa", "qa")])];
    expect(computeSearchResults(ctx({ workspaces }), "workspace")).toEqual([]);
    expect(computeSearchResults(ctx({ items: [makeItem("skill:qa", "qa")] }), "global")).toEqual([]);
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
