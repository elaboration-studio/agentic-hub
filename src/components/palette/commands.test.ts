import { beforeEach, describe, expect, it, vi } from "vitest";

const { requestApply } = vi.hoisted(() => ({
  requestApply: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/state/apply", () => ({
  useApplyStore: { getState: () => ({ request: requestApply }) },
}));

vi.mock("@/ipc", () => ({
  openPath: vi.fn(),
  revealPath: vi.fn(),
  readCapabilityBody: vi.fn().mockResolvedValue("command body text"),
  copyText: vi.fn().mockResolvedValue(undefined),
  pasteToFrontmost: vi.fn().mockResolvedValue({ pasted: true, needsPermission: false }),
  recordCommandPaletteUsage: vi.fn().mockResolvedValue(undefined),
  setWatcherEnabled: vi.fn().mockResolvedValue(undefined),
  emitHubWatcherChanged: vi.fn().mockResolvedValue(undefined),
}));

import {
  copyText,
  emitHubWatcherChanged,
  openPath,
  pasteToFrontmost,
  readCapabilityBody,
  recordCommandPaletteUsage,
  revealPath,
  setWatcherEnabled,
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
import { key } from "@/shared";
import {
  SEARCH_MODES,
  computeCapabilityToolResults,
  computeHubResults,
  computeSearchResults,
  computeSuiteToolResults,
  searchModeFromShortcut,
  shouldDismissPaletteAfterRun,
  type CapabilityOwnership,
  type CapabilityToolsContext,
  type ProviderContext,
  type WorkspaceInventoryEntry,
} from "./commands";

const mocked = {
  openPath: vi.mocked(openPath),
  revealPath: vi.mocked(revealPath),
  requestApply,
  readCapabilityBody: vi.mocked(readCapabilityBody),
  copyText: vi.mocked(copyText),
  pasteToFrontmost: vi.mocked(pasteToFrontmost),
  recordCommandPaletteUsage: vi.mocked(recordCommandPaletteUsage),
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
    hooksDir: null,
    commandsPath: enabled ? "/commands" : null,
  };
}

function makeSettings(overrides: Partial<Record<ToolId, boolean>> = {}): Settings {
  return {
    sources: [],
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
    paletteQuickSearchShortcuts: {
      allResources: "Cmd+Alt+Ctrl+A",
      skills: "Cmd+Alt+Ctrl+S",
      commands: "Cmd+Alt+Ctrl+C",
    },
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
      claude: toolSettings(overrides.claude ?? true),
      cursor: toolSettings(overrides.cursor ?? true),
      openclaw: toolSettings(overrides.openclaw ?? false),
      openstandard: toolSettings(overrides.openstandard ?? false),
      kiro: toolSettings(overrides.kiro ?? false),
      copilot: toolSettings(overrides.copilot ?? false),
      antigravity: toolSettings(overrides.antigravity ?? false),
      grok: toolSettings(overrides.grok ?? false),
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
    enterCapabilityTools: vi.fn(),
    locate: vi.fn(),
    ...overrides,
  };
}

function state(tool: ToolId, itemId: string, on: boolean): ToolCapabilityState {
  return {
    tool,
    itemId,
    targetPath: `/${tool}/${itemId}`,
    state: on ? "enabled" : "disabled",
    currentLinkTarget: on ? `/shared/${itemId}` : null,
  };
}

function capCtx(overrides: Partial<CapabilityToolsContext> = {}): CapabilityToolsContext {
  return {
    settings: makeSettings(),
    query: "",
    itemId: "skill:tdd",
    itemName: "tdd",
    item: makeItem("skill:tdd", "tdd"),
    inspected: true,
    currentMap: new Map<string, ToolCapabilityState>(),
    ownership: new Map<string, CapabilityOwnership>(),
    toggleCapability: vi.fn(),
    toggleCapabilityAll: vi.fn(),
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

    // "skills" matches the skill mode row by title.
    const skillRows = computeHubResults(ctx({ items, query: "skills" }));
    expect(skillRows.map((r) => r.title)).toEqual(["Search skills"]);
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
  it("a kind mode returns only that kind; Enter drills in, Alt+Enter opens the file", () => {
    const items = [
      makeItem("skill:review", "review"),
      makeItem("agent:review-bot", "review-bot", "agent"),
    ];
    const enterCapabilityTools = vi.fn();
    const rows = computeSearchResults(ctx({ items, query: "review", enterCapabilityTools }), "skill");

    expect(rows.map((r) => r.id)).toEqual(["resource:skill:review"]);
    // Enter drills into the per-tool toggle panel (stays open), no file opened.
    expect(rows[0].dismissOnRun).toBe(false);
    rows[0].run();
    expect(enterCapabilityTools).toHaveBeenCalledWith(items[0], "skill");
    expect(mocked.openPath).not.toHaveBeenCalled();

    // Alt+Enter opens the original file in the editor.
    rows[0].altRun!();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/skills/review/SKILL.md", undefined);
  });

  it("the all mode drills in with the 'all' fromMode so Back returns there", () => {
    const items = [makeItem("rule:style", "style", "rule")];
    const enterCapabilityTools = vi.fn();
    const rows = computeSearchResults(ctx({ items, query: "style", enterCapabilityTools }), "all");
    rows[0].run();
    expect(enterCapabilityTools).toHaveBeenCalledWith(items[0], "all");
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
    expect(mocked.pasteToFrontmost).not.toHaveBeenCalled();
    expect(mocked.recordCommandPaletteUsage).toHaveBeenCalledWith(
      "command:git/commit.md",
      false,
    );
    expect(mocked.openPath).not.toHaveBeenCalled();
  });

  it("Enter also pastes when pasteIntoFocused is enabled", async () => {
    const settings = { ...makeSettings(), pasteIntoFocused: true };
    const items = [makeCommand("command:git/commit.md", "commit", "git/commit.md")];
    const row = computeSearchResults(ctx({ settings, items, query: "commit" }), "command")[0];

    await row.run();
    expect(mocked.copyText).toHaveBeenCalledWith("command body text");
    expect(mocked.pasteToFrontmost).toHaveBeenCalled();
    expect(mocked.recordCommandPaletteUsage).toHaveBeenCalledWith(
      "command:git/commit.md",
      true,
    );
  });

  it("Alt+Enter opens the source file for editing without recording usage", async () => {
    const items = [makeCommand("command:git/commit.md", "commit", "git/commit.md")];
    const row = computeSearchResults(ctx({ items, query: "commit" }), "command")[0];

    await row.altRun!();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/commands/git/commit.md", undefined);
    expect(mocked.copyText).not.toHaveBeenCalled();
    expect(mocked.recordCommandPaletteUsage).not.toHaveBeenCalled();
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

  it("running an apply row routes through the apply store preview flow", () => {
    const rows = computeSuiteToolResults(makeSettings(), "", "s1", "Backend");
    rows.find((r) => r.title === "Apply to Cursor")!.run();
    expect(mocked.requestApply).toHaveBeenCalledWith("cursor", "s1", "Backend");
  });
});

describe("shouldDismissPaletteAfterRun", () => {
  it("keeps the palette open when suite apply needs confirmation", () => {
    expect(shouldDismissPaletteAfterRun({ dismissOnRun: undefined }, true)).toBe(false);
  });

  it("dismisses terminal rows when no apply confirm is pending", () => {
    expect(shouldDismissPaletteAfterRun({ dismissOnRun: undefined }, false)).toBe(true);
  });

  it("respects dismissOnRun false for drill-in rows", () => {
    expect(shouldDismissPaletteAfterRun({ dismissOnRun: false }, false)).toBe(false);
  });

  it("dismisses when the selected item is missing", () => {
    expect(shouldDismissPaletteAfterRun(undefined, false)).toBe(true);
  });
});

describe("capability-tools view", () => {
  // Only tools with a projection target (a currentMap entry) get a row.
  const fullMap = new Map<string, ToolCapabilityState>([
    [key("codex", "skill:tdd"), state("codex", "skill:tdd", true)],
    [key("claude", "skill:tdd"), state("claude", "skill:tdd", false)],
    [key("cursor", "skill:tdd"), state("cursor", "skill:tdd", false)],
  ]);

  it("shows a loading row (plus action rows) until inspected", () => {
    const rows = computeCapabilityToolResults(capCtx({ inspected: false }));
    expect(rows.map((r) => r.title)).toEqual([
      "Loading tool states…",
      "Open in editor",
      "Reveal in Finder",
    ]);
  });

  it("lists one row per projectable enabled tool with on/off state", () => {
    const rows = computeCapabilityToolResults(capCtx({ currentMap: fullMap }));
    const toolRows = rows.filter((r) => r.group === "Tool" && !r.id.startsWith("captool-all"));
    expect(toolRows.map((r) => [r.title, r.state])).toEqual([
      ["Codex", "on"],
      ["Claude", "off"],
      ["Cursor", "off"],
    ]);
  });

  it("excludes tools without a projection target (e.g. a hook's non-targets)", () => {
    const partial = new Map<string, ToolCapabilityState>([
      [key("claude", "hook:x"), state("claude", "hook:x", true)],
    ]);
    const rows = computeCapabilityToolResults(
      capCtx({ itemId: "hook:x", itemName: "x", item: makeItem("hook:x", "x", "hook"), currentMap: partial }),
    );
    const toolRows = rows.filter((r) => r.group === "Tool" && !r.id.startsWith("captool-all"));
    expect(toolRows.map((r) => r.title)).toEqual(["Claude"]);
  });

  it("toggling a tool row calls toggleCapability for that tool", () => {
    const toggleCapability = vi.fn();
    const rows = computeCapabilityToolResults(capCtx({ currentMap: fullMap, toggleCapability }));
    const claude = rows.find((r) => r.title === "Claude")!;
    expect(claude.dismissOnRun).toBe(false);
    claude.run();
    expect(toggleCapability).toHaveBeenCalledWith("claude", "skill:tdd");
  });

  it("renders a locked row that does not toggle when a suite owns the cell", () => {
    const toggleCapability = vi.fn();
    const ownership = new Map<string, CapabilityOwnership>([
      [key("codex", "skill:tdd"), { suiteName: "Backend" }],
    ]);
    const rows = computeCapabilityToolResults(
      capCtx({ currentMap: fullMap, ownership, toggleCapability }),
    );
    const codex = rows.find((r) => r.title === "Codex")!;
    expect(codex.state).toBe("locked");
    codex.run();
    expect(toggleCapability).not.toHaveBeenCalled();
  });

  it("the aggregate row enables for all when not every tool is on", () => {
    const toggleCapabilityAll = vi.fn();
    const rows = computeCapabilityToolResults(capCtx({ currentMap: fullMap, toggleCapabilityAll }));
    const all = rows.find((r) => r.id.startsWith("captool-all"))!;
    expect(all.title).toBe("Enable for all tools");
    all.run();
    expect(toggleCapabilityAll).toHaveBeenCalledWith("skill:tdd", true);
  });

  it("the aggregate row disables for all when every unlocked tool is already on", () => {
    const allOn = new Map<string, ToolCapabilityState>([
      [key("codex", "skill:tdd"), state("codex", "skill:tdd", true)],
      [key("claude", "skill:tdd"), state("claude", "skill:tdd", true)],
      [key("cursor", "skill:tdd"), state("cursor", "skill:tdd", true)],
    ]);
    const toggleCapabilityAll = vi.fn();
    const rows = computeCapabilityToolResults(capCtx({ currentMap: allOn, toggleCapabilityAll }));
    const all = rows.find((r) => r.id.startsWith("captool-all"))!;
    expect(all.title).toBe("Disable for all tools");
    all.run();
    expect(toggleCapabilityAll).toHaveBeenCalledWith("skill:tdd", false);
  });

  it("the action rows open and reveal the original file", () => {
    const rows = computeCapabilityToolResults(capCtx({ currentMap: fullMap }));
    rows.find((r) => r.title === "Open in editor")!.run();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/skills/tdd/SKILL.md", undefined);
    rows.find((r) => r.title === "Reveal in Finder")!.run();
    expect(mocked.revealPath).toHaveBeenCalledWith("/shared/skills/tdd/SKILL.md");
  });

  it("hides the aggregate and action rows while filtering by a query", () => {
    const rows = computeCapabilityToolResults(capCtx({ currentMap: fullMap, query: "claude" }));
    expect(rows.map((r) => r.title)).toEqual(["Claude"]);
  });
});
