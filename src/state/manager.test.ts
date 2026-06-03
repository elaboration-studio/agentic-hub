import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri IPC boundary and Sonner toasts so the store logic runs in
// plain Node. These factories are hoisted by Vitest above the imports.
vi.mock("@/ipc", () => ({
  loadSettings: vi.fn(),
  scan: vi.fn(),
  scanWorkspace: vi.fn(),
  inspect: vi.fn(),
  plan: vi.fn(),
  apply: vi.fn(),
  syncRules: vi.fn(),
  syncHooks: vi.fn(),
  setWatcherEnabled: vi.fn(),
  suiteOwnership: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  apply,
  inspect,
  loadSettings,
  plan,
  scan,
  scanWorkspace,
  setWatcherEnabled,
  suiteOwnership,
  syncHooks,
  syncRules,
} from "@/ipc";
import { toast } from "sonner";
import type {
  ApplyResult,
  CapabilityItem,
  InspectResult,
  LinkState,
  Settings,
  ToolCapabilityState,
  ToolId,
  ToolSettings,
} from "@/types";
import { useManagerStore } from "./manager";

const mocked = {
  loadSettings: vi.mocked(loadSettings),
  scan: vi.mocked(scan),
  scanWorkspace: vi.mocked(scanWorkspace),
  inspect: vi.mocked(inspect),
  plan: vi.mocked(plan),
  apply: vi.mocked(apply),
  syncRules: vi.mocked(syncRules),
  syncHooks: vi.mocked(syncHooks),
  setWatcherEnabled: vi.mocked(setWatcherEnabled),
  suiteOwnership: vi.mocked(suiteOwnership),
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
    sources: [{ id: "default", label: "Default", path: "/shared" }],
    sharedRoot: "/shared",
    suitesPath: null,
    watcherEnabled: true,
    editor: { kind: "default", customApp: null },
    paletteShortcut: "Cmd+Alt+A",
    tools: {
      codex: toolSettings(overrides.codex ?? true),
      claude: toolSettings(overrides.claude ?? false),
      cursor: toolSettings(overrides.cursor ?? true),
      openclaw: toolSettings(overrides.openclaw ?? false),
    },
  };
}

function makeItem(id: string): CapabilityItem {
  return {
    id,
    kind: "skill",
    name: id,
    sourcePath: `/shared/skills/${id}`,
    relativePath: id,
    sourceId: "default",
    sourceLabel: "Default",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function makeState(tool: ToolId, itemId: string, state: LinkState): ToolCapabilityState {
  return { tool, itemId, targetPath: `/${tool}/${itemId}`, state, currentLinkTarget: null };
}

const APPLY_RESULT: ApplyResult = {
  created: 1,
  removed: 0,
  replaced: 0,
  refreshed: 0,
  skipped: 0,
  errors: [],
};

// Default happy-path scan: one skill, enabled in codex, disabled in cursor.
function seedHappyPath(result?: Partial<InspectResult>) {
  const item = makeItem("skill:a");
  const inspectResult: InspectResult = {
    states: [makeState("codex", "skill:a", "enabled"), makeState("cursor", "skill:a", "disabled")],
    adapterStatuses: [
      { tool: "codex", available: true, unavailableReason: null },
      { tool: "cursor", available: true, unavailableReason: null },
    ],
    ...result,
  };
  mocked.loadSettings.mockResolvedValue(makeSettings());
  mocked.scan.mockResolvedValue({ items: [item], errors: [] });
  mocked.inspect.mockResolvedValue(inspectResult);
  mocked.suiteOwnership.mockResolvedValue([]);
}

beforeEach(() => {
  vi.clearAllMocks();
  useManagerStore.setState(useManagerStore.getInitialState(), true);
});

describe("manager store — refresh", () => {
  it("populates data, seeds the desired map from current state, and derives tools", async () => {
    seedHappyPath();

    await useManagerStore.getState().refresh();
    const s = useManagerStore.getState();

    expect(s.status).toBe("ready");
    expect(s.data?.items).toHaveLength(1);
    expect(s.desired["codex::skill:a"]).toBe(true);
    expect(s.desired["cursor::skill:a"]).toBe(false);
    expect(s.tools.map((t) => t.id)).toEqual(["codex", "cursor"]);
    expect(s.pendingKeys).toEqual([]);
  });

  it("re-syncs desired and ownership when refreshed after an external apply", async () => {
    // Initial state: cursor disabled, no suite owns the cell.
    seedHappyPath();
    await useManagerStore.getState().refresh();
    expect(useManagerStore.getState().desired["cursor::skill:a"]).toBe(false);
    expect(useManagerStore.getState().ownership.size).toBe(0);

    // A suite is applied externally (Suites page / palette) → the backend emits
    // `sources-changed`, App calls refresh(). Disk now shows cursor enabled and
    // owned by a suite. The Manager must reflect both.
    mocked.inspect.mockResolvedValue({
      states: [makeState("codex", "skill:a", "enabled"), makeState("cursor", "skill:a", "enabled")],
      adapterStatuses: [
        { tool: "codex", available: true, unavailableReason: null },
        { tool: "cursor", available: true, unavailableReason: null },
      ],
    });
    mocked.suiteOwnership.mockResolvedValue([
      { tool: "cursor", itemId: "skill:a", suiteId: "s1", suiteName: "Backend", fromBase: false },
    ]);

    await useManagerStore.getState().refresh();
    const s = useManagerStore.getState();

    expect(s.desired["cursor::skill:a"]).toBe(true);
    expect(s.ownership.get("cursor::skill:a")).toEqual({ suiteName: "Backend", fromBase: false });
    expect(s.pendingKeys).toEqual([]);
  });

  it("sets status=error and the message when a load step throws", async () => {
    mocked.loadSettings.mockRejectedValue(new Error("config unreadable"));

    await useManagerStore.getState().refresh();
    const s = useManagerStore.getState();

    expect(s.status).toBe("error");
    expect(s.error).toContain("config unreadable");
    expect(s.data).toBeNull();
  });
});

describe("manager store — loadWorkspace (read-only inventory)", () => {
  // Local workspace items are tagged with this prefix so they never collide
  // with a global resource of the same relative path. Mirrors `manager.ts`.
  const WS = "ws::";

  // No globally-applied resources: empty shared scan + inspect.
  function noGlobal() {
    mocked.scan.mockResolvedValue({ items: [], errors: [] });
    mocked.inspect.mockResolvedValue({ states: [], adapterStatuses: [] });
  }

  it("populates a read-only inventory with fixed workspace tools and no pending keys", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    noGlobal();
    mocked.scanWorkspace.mockResolvedValue({
      items: [makeItem("skill:a")],
      states: [
        makeState("cursor", "skill:a", "enabled"),
        makeState("claude", "skill:a", "enabled"),
      ],
      errors: [],
    });

    await useManagerStore.getState().loadWorkspace("ws-1");
    const s = useManagerStore.getState();

    expect(s.status).toBe("ready");
    expect(s.readOnly).toBe(true);
    expect(s.data?.items).toHaveLength(1);
    // Workspace columns are the three supported tools regardless of global
    // enabled settings (claude is disabled in makeSettings but still shown).
    expect(s.tools.map((t) => t.id)).toEqual(["codex", "claude", "cursor"]);
    // Local resources are namespaced so they stay distinct from globals.
    expect(s.desired[`cursor::${WS}skill:a`]).toBe(true);
    expect(s.desired[`claude::${WS}skill:a`]).toBe(true);
    // Desired mirrors current, so there is nothing to apply.
    expect(s.pendingKeys).toEqual([]);
  });

  it("merges globally-applied resources, keeping only enabled global states", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    // Global scan finds two skills; only the codex-enabled one applies. The
    // disabled global projection must be dropped from the inventory.
    mocked.scan.mockResolvedValue({
      items: [makeItem("skill:global-on"), makeItem("skill:global-off")],
      errors: [],
    });
    mocked.inspect.mockResolvedValue({
      states: [
        makeState("codex", "skill:global-on", "enabled"),
        makeState("cursor", "skill:global-off", "disabled"),
      ],
      adapterStatuses: [],
    });
    mocked.scanWorkspace.mockResolvedValue({
      items: [makeItem("skill:local")],
      states: [makeState("cursor", "skill:local", "enabled")],
      errors: [],
    });

    await useManagerStore.getState().loadWorkspace("ws-1");
    const s = useManagerStore.getState();

    const ids = (s.data?.items ?? []).map((i) => i.id).sort();
    // global-off is excluded; global-on (global id, un-prefixed) and the local
    // skill (namespaced) both survive.
    expect(ids).toEqual(["skill:global-on", `${WS}skill:local`]);
    expect(s.desired["codex::skill:global-on"]).toBe(true);
    expect(s.desired[`cursor::${WS}skill:local`]).toBe(true);
    expect(s.desired["cursor::skill:global-off"]).toBeUndefined();
    expect(s.pendingKeys).toEqual([]);
  });

  it("read-only mode rejects toggles", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    noGlobal();
    mocked.scanWorkspace.mockResolvedValue({
      items: [makeItem("skill:a")],
      states: [makeState("cursor", "skill:a", "enabled")],
      errors: [],
    });
    await useManagerStore.getState().loadWorkspace("ws-1");

    useManagerStore.getState().toggle("cursor", `${WS}skill:a`);

    expect(useManagerStore.getState().desired[`cursor::${WS}skill:a`]).toBe(true);
    expect(useManagerStore.getState().pendingKeys).toEqual([]);
  });

  it("refresh clears the read-only flag set by loadWorkspace", async () => {
    mocked.loadSettings.mockResolvedValue(makeSettings());
    noGlobal();
    mocked.scanWorkspace.mockResolvedValue({ items: [], states: [], errors: [] });
    await useManagerStore.getState().loadWorkspace("ws-1");
    expect(useManagerStore.getState().readOnly).toBe(true);

    seedHappyPath();
    await useManagerStore.getState().refresh();

    expect(useManagerStore.getState().readOnly).toBe(false);
  });
});

describe("manager store — staging", () => {
  it("toggle flips the desired value and marks the key as pending", async () => {
    seedHappyPath();
    await useManagerStore.getState().refresh();

    useManagerStore.getState().toggle("codex", "skill:a");
    const s = useManagerStore.getState();

    expect(s.desired["codex::skill:a"]).toBe(false);
    expect(s.pendingKeys).toContain("codex::skill:a");
  });

  it("toggle is a no-op for a tool/item pair that is not in the current map", async () => {
    seedHappyPath();
    await useManagerStore.getState().refresh();

    useManagerStore.getState().toggle("claude", "skill:a");
    const s = useManagerStore.getState();

    expect(s.desired["claude::skill:a"]).toBeUndefined();
    expect(s.pendingKeys).toEqual([]);
  });

  it("loads suite ownership on refresh and locks owned cells against toggle", async () => {
    seedHappyPath();
    mocked.suiteOwnership.mockResolvedValue([
      { tool: "codex", itemId: "skill:a", suiteId: "s1", suiteName: "Backend", fromBase: false },
    ]);
    await useManagerStore.getState().refresh();

    expect(useManagerStore.getState().ownership.get("codex::skill:a")).toEqual({
      suiteName: "Backend",
      fromBase: false,
    });

    // A suite owns this cell, so manual toggle is a no-op (locked).
    useManagerStore.getState().toggle("codex", "skill:a");
    expect(useManagerStore.getState().pendingKeys).toEqual([]);

    // Batch toggles skip owned cells too.
    useManagerStore.getState().toggleMany("codex", ["skill:a"], false);
    expect(useManagerStore.getState().pendingKeys).toEqual([]);
  });

  it("toggleMany stages several keys and resetDesired restores the seeded state", async () => {
    seedHappyPath();
    await useManagerStore.getState().refresh();

    useManagerStore.getState().toggleMany("codex", ["skill:a"], false);
    expect(useManagerStore.getState().pendingKeys).toContain("codex::skill:a");

    useManagerStore.getState().resetDesired();
    const s = useManagerStore.getState();
    expect(s.pendingKeys).toEqual([]);
    expect(s.desired["codex::skill:a"]).toBe(true);
  });
});

describe("manager store — apply pipeline", () => {
  it("requestApply with a foreign_file enable surfaces a conflict instead of applying", async () => {
    const item = makeItem("skill:a");
    mocked.loadSettings.mockResolvedValue(makeSettings());
    mocked.scan.mockResolvedValue({ items: [item], errors: [] });
    mocked.inspect.mockResolvedValue({
      states: [makeState("codex", "skill:a", "foreign_file")],
      adapterStatuses: [{ tool: "codex", available: true, unavailableReason: null }],
    });
    await useManagerStore.getState().refresh();

    // Enabling a target blocked by a real file is the conflict case.
    useManagerStore.getState().toggle("codex", "skill:a");
    useManagerStore.getState().requestApply();
    const s = useManagerStore.getState();

    expect(s.conflicts).toHaveLength(1);
    expect(s.conflicts?.[0].targetPath).toBe("/codex/skill:a");
    expect(mocked.plan).not.toHaveBeenCalled();
  });

  it("requestApply with no conflicts runs plan -> apply -> sync and toasts a summary", async () => {
    seedHappyPath();
    await useManagerStore.getState().refresh();
    mocked.plan.mockResolvedValue([
      {
        tool: "codex",
        itemId: "skill:a",
        targetRoot: "/codex",
        targetPath: "/codex/skill:a",
        sourcePath: "/shared/skills/skill:a",
        kind: "create_link",
        reason: "",
        force: false,
      },
    ]);
    mocked.apply.mockResolvedValue(APPLY_RESULT);
    mocked.syncRules.mockResolvedValue({} as never);
    mocked.syncHooks.mockResolvedValue({} as never);

    useManagerStore.getState().toggle("codex", "skill:a");
    useManagerStore.getState().requestApply();

    await vi.waitFor(() => expect(mocked.apply).toHaveBeenCalled());
    expect(mocked.plan).toHaveBeenCalledWith(
      "codex",
      expect.any(Array),
      { "skill:a": false },
      false,
    );
    expect(mocked.syncRules).toHaveBeenCalled();
    expect(mocked.syncHooks).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalled();
    expect(useManagerStore.getState().applying).toBe(false);
  });

  it("resolveConflicts(true) applies with force and clears the conflict prompt", async () => {
    seedHappyPath();
    await useManagerStore.getState().refresh();
    mocked.plan.mockResolvedValue([]);
    mocked.syncRules.mockResolvedValue({} as never);
    mocked.syncHooks.mockResolvedValue({} as never);

    useManagerStore.getState().toggle("codex", "skill:a");
    useManagerStore.setState({ conflicts: [{ toolLabel: "Codex", name: "skill:a", targetPath: "/x" }] });

    useManagerStore.getState().resolveConflicts(true);

    await vi.waitFor(() => expect(mocked.plan).toHaveBeenCalled());
    expect(mocked.plan).toHaveBeenCalledWith("codex", expect.any(Array), expect.any(Object), true);
    expect(useManagerStore.getState().conflicts).toBeNull();
  });
});

describe("manager store — watcher", () => {
  it("toggleWatching persists the new value through IPC", async () => {
    mocked.setWatcherEnabled.mockResolvedValue(undefined);

    await useManagerStore.getState().toggleWatching(false);

    expect(mocked.setWatcherEnabled).toHaveBeenCalledWith(false);
    expect(useManagerStore.getState().watching).toBe(false);
  });

  it("toggleWatching reverts and toasts when IPC fails", async () => {
    mocked.setWatcherEnabled.mockRejectedValue(new Error("nope"));

    await useManagerStore.getState().toggleWatching(false);

    expect(useManagerStore.getState().watching).toBe(true);
    expect(toast.error).toHaveBeenCalled();
  });
});
