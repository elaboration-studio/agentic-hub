import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri IPC boundary so the registry + store logic runs in plain Node.
// `openPath` is invoked by the resource provider; the nav provider uses
// `emitHubNavigate` + `showMain`.
vi.mock("@/ipc", () => ({
  loadSettings: vi.fn(),
  scan: vi.fn(),
  openPath: vi.fn(),
  emitHubNavigate: vi.fn(() => Promise.resolve()),
  showMain: vi.fn(() => Promise.resolve()),
}));

import { emitHubNavigate, loadSettings, openPath, scan } from "@/ipc";
import type { CapabilityItem, Settings } from "@/types";
import { getInitialState, usePaletteStore } from "./palette";

const mocked = {
  loadSettings: vi.mocked(loadSettings),
  scan: vi.mocked(scan),
  openPath: vi.mocked(openPath),
  emitHubNavigate: vi.mocked(emitHubNavigate),
};

function makeSettings(): Settings {
  return {
    sources: [{ id: "default", label: "Default", path: "/shared" }],
    sharedRoot: "/shared",
    suitesPath: null,
    watcherEnabled: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    tools: {
      codex: tool(),
      claude: tool(),
      cursor: tool(),
      openclaw: tool(),
    },
  };
}

function tool() {
  return {
    enabled: true,
    skillsPath: "/skills",
    agentsPath: "/agents",
    rulesPath: "/rules",
    instructionsPath: null,
    hooksEnabled: false,
    hooksFile: null,
  };
}

function item(partial: Partial<CapabilityItem> & Pick<CapabilityItem, "id" | "name">): CapabilityItem {
  return {
    kind: "skill",
    sourcePath: `/shared/skills/${partial.name}`,
    relativePath: partial.name,
    sourceId: "default",
    sourceLabel: "Default",
    valid: true,
    validationErrors: [],
    ...partial,
  };
}

const ITEMS: CapabilityItem[] = [
  item({ id: "skill:manager-helper", name: "manager-helper" }),
  item({ id: "skill:repo-research", name: "repo-research" }),
  item({ id: "agent:reviewer", name: "reviewer", kind: "agent", sourcePath: "/shared/agents/reviewer.md" }),
];

async function loadStore() {
  mocked.loadSettings.mockResolvedValue(makeSettings());
  mocked.scan.mockResolvedValue({ items: ITEMS, errors: [] });
  await usePaletteStore.getState().load();
}

beforeEach(() => {
  vi.clearAllMocks();
  // Merge (not replace) so the action functions survive the reset.
  usePaletteStore.setState(getInitialState());
});

describe("palette store", () => {
  it("loads settings and items, showing nav commands on an empty query", async () => {
    await loadStore();
    const s = usePaletteStore.getState();
    expect(s.status).toBe("ready");
    expect(s.items).toHaveLength(3);
    // Empty query: only the three navigation commands are visible.
    expect(s.results.map((r) => r.group)).toEqual(["Navigate", "Navigate", "Navigate"]);
    expect(s.selectedIndex).toBe(0);
  });

  it("filters resources by name, path, or source on query", async () => {
    await loadStore();
    usePaletteStore.getState().setQuery("repo");
    const s = usePaletteStore.getState();
    expect(s.results).toHaveLength(1);
    expect(s.results[0].title).toBe("repo-research");
    expect(s.results[0].group).toBe("Skill");
  });

  it("orders matching resources before navigation commands", async () => {
    await loadStore();
    usePaletteStore.getState().setQuery("manager");
    const groups = usePaletteStore.getState().results.map((r) => r.group);
    // Resource "manager-helper" precedes the "Open Manager" nav command.
    expect(groups[0]).toBe("Skill");
    expect(groups).toContain("Navigate");
  });

  it("wraps selection around both ends", async () => {
    await loadStore();
    const { move, setSelected } = usePaletteStore.getState();
    // 3 nav results on empty query.
    setSelected(0);
    move(-1);
    expect(usePaletteStore.getState().selectedIndex).toBe(2);
    move(1);
    expect(usePaletteStore.getState().selectedIndex).toBe(0);
  });

  it("opens the original file with the configured editor for a resource", async () => {
    await loadStore();
    usePaletteStore.getState().setQuery("repo-research");
    await usePaletteStore.getState().runSelected();
    expect(mocked.openPath).toHaveBeenCalledWith("/shared/skills/repo-research/SKILL.md", undefined);
  });

  it("routes navigation commands back to the main window", async () => {
    await loadStore();
    usePaletteStore.getState().setQuery("Open Suites");
    const s = usePaletteStore.getState();
    expect(s.results[0].group).toBe("Navigate");
    await s.runSelected();
    expect(mocked.emitHubNavigate).toHaveBeenCalledWith("suites");
  });
});
