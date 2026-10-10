import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listSuites: vi.fn(),
  createSuite: vi.fn(),
  updateSuite: vi.fn(),
  deleteSuite: vi.fn(),
  setBaseSuite: vi.fn(),
  agentVersion: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  agentVersion,
  createSuite,
  deleteSuite,
  listSuites,
  setBaseSuite,
  updateSuite,
} from "@/ipc";
import { toast } from "sonner";
import type {
  CapabilityItem,
  SourceRef,
  SuiteDefinition,
  ToolCapabilityState,
} from "@/types";
import {
  draftIncludesItem,
  suiteRefMatchesItem,
  useSuitesStore,
} from "./suites";
import { EMPTY_AGENT } from "./agentDraft";

const mocked = {
  listSuites: vi.mocked(listSuites),
  createSuite: vi.mocked(createSuite),
  updateSuite: vi.mocked(updateSuite),
  deleteSuite: vi.mocked(deleteSuite),
  setBaseSuite: vi.mocked(setBaseSuite),
  agentVersion: vi.mocked(agentVersion),
};

const SRC: SourceRef = { relHome: "~/.agentic", folder: ".agentic" };

function makeSuite(overrides: Partial<SuiteDefinition> = {}): SuiteDefinition {
  return {
    id: "s1",
    name: "Backend",
    description: null,
    capabilities: [{ cap: "skill:a", source: SRC }],
    isBase: false,
    createdAt: "2026-01-01",
    updatedAt: "2026-01-01",
    ...overrides,
  };
}

function makeItem(id: string, source: SourceRef = SRC): CapabilityItem {
  return {
    id,
    kind: "skill",
    name: id,
    sourcePath: `/src/${id}`,
    relativePath: id.replace("skill:", ""),
    sourceId: "default",
    sourceLabel: "Default",
    source,
    valid: true,
    validationErrors: [],
  };
}

function makeState(itemId: string, state: ToolCapabilityState["state"]): ToolCapabilityState {
  return {
    tool: "codex",
    itemId,
    state,
    targetPath: `/target/${itemId}`,
    currentLinkTarget: null,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocked.agentVersion.mockResolvedValue("abc123def456");
  useSuitesStore.setState(useSuitesStore.getInitialState(), true);
});

describe("suites store — loading", () => {
  it("reload populates the suite list from IPC", async () => {
    mocked.listSuites.mockResolvedValue([makeSuite()]);

    await useSuitesStore.getState().reload();

    expect(useSuitesStore.getState().suites).toHaveLength(1);
  });

  it("reload toasts an error and leaves the list empty when IPC throws", async () => {
    mocked.listSuites.mockRejectedValue(new Error("disk gone"));

    await useSuitesStore.getState().reload();

    expect(toast.error).toHaveBeenCalledWith("disk gone");
    expect(useSuitesStore.getState().suites).toEqual([]);
  });
});

describe("suites store — draft editing", () => {
  it("matches qualified suite references by portable source identity", () => {
    const item = makeItem("skill:a", { relHome: "~/team-agentic", folder: "team-agentic" });

    expect(
      suiteRefMatchesItem(
        { cap: "skill:a", source: { relHome: "~/missing", folder: "team-agentic" } },
        item,
      ),
    ).toBe(true);
    expect(
      suiteRefMatchesItem(
        { cap: "skill:a", source: { relHome: "~/missing", folder: "other" } },
        item,
      ),
    ).toBe(false);
  });

  it("does not include a live same-id item from another source until selected", () => {
    const missingRef = {
      cap: "skill:a",
      source: { relHome: "~/team-agentic", folder: "team-agentic" },
    };
    const liveItem = makeItem("skill:a", {
      relHome: "~/personal-agentic",
      folder: "personal-agentic",
    });
    useSuitesStore.setState({
      suites: [makeSuite({ capabilities: [missingRef] })],
    });

    useSuitesStore.getState().selectSuite("s1");

    expect(
      draftIncludesItem(
        useSuitesStore.getState().draft?.capabilities ?? [],
        liveItem,
      ),
    ).toBe(false);

    useSuitesStore.getState().setCapabilities([liveItem], true);

    expect(
      draftIncludesItem(
        useSuitesStore.getState().draft?.capabilities ?? [],
        liveItem,
      ),
    ).toBe(true);
    expect(useSuitesStore.getState().draft?.capabilities).toEqual([
      missingRef,
      { cap: "skill:a", source: liveItem.source },
    ]);
  });

  it("startCreate opens a blank draft and clears any selection", () => {
    useSuitesStore.setState({ selectedId: "s1" });

    useSuitesStore.getState().startCreate();
    const s = useSuitesStore.getState();

    expect(s.isCreating).toBe(true);
    expect(s.selectedId).toBeUndefined();
    expect(s.draft).toEqual({ name: "", description: "", capabilities: [], agent: EMPTY_AGENT });
  });

  it("startCreateFromCurrent includes only enabled Hub-managed resources for the selected tool", () => {
    const items = [
      makeItem("skill:enabled"),
      makeItem("skill:disabled"),
      { ...makeItem("skill:installed"), sourceId: "installed:codex" },
      { ...makeItem("hook:internal"), sourceId: "agentic-hub" },
    ];
    const states = [
      makeState("skill:enabled", "enabled"),
      makeState("skill:disabled", "disabled"),
      makeState("skill:installed", "enabled"),
      makeState("hook:internal", "enabled"),
      { ...makeState("skill:other-tool", "enabled"), tool: "claude" as const },
    ];

    useSuitesStore
      .getState()
      .startCreateFromCurrent("codex", items, states, new Set(["skill:installed"]));

    expect(useSuitesStore.getState().draft).toEqual({
      name: "",
      description: "",
      capabilities: [{ cap: "skill:enabled", source: SRC }],
      agent: EMPTY_AGENT,
    });
  });

  it("selectSuite loads the chosen suite into the draft, normalizing null description", () => {
    useSuitesStore.setState({ suites: [makeSuite({ description: null })] });

    useSuitesStore.getState().selectSuite("s1");
    const s = useSuitesStore.getState();

    expect(s.selectedId).toBe("s1");
    expect(s.draft).toEqual({
      name: "Backend",
      description: "",
      capabilities: [{ cap: "skill:a", source: SRC }],
      agent: EMPTY_AGENT,
    });
  });

  it("selectSuite is a no-op for an unknown id", () => {
    useSuitesStore.setState({ suites: [makeSuite()] });

    useSuitesStore.getState().selectSuite("missing");

    expect(useSuitesStore.getState().selectedId).toBeUndefined();
  });

  it("setCapabilities adds source-aware refs without duplicating, and removes when off", () => {
    const itemA = makeItem("skill:a");
    const itemB = makeItem("skill:b");
    useSuitesStore.setState({
      draft: {
        name: "x",
        description: "",
        capabilities: [{ cap: "skill:a", source: SRC }],
        agent: EMPTY_AGENT,
      },
    });

    useSuitesStore.getState().setCapabilities([itemA, itemB], true);
    expect(useSuitesStore.getState().draft?.capabilities).toEqual([
      { cap: "skill:a", source: SRC },
      { cap: "skill:b", source: SRC },
    ]);

    useSuitesStore.getState().setCapabilities([itemA], false);
    expect(useSuitesStore.getState().draft?.capabilities).toEqual([
      { cap: "skill:b", source: SRC },
    ]);
  });

  it("removeCapabilities drops only the requested stale references", () => {
    const liveRef = {
      cap: "skill:a",
      source: { relHome: "~/personal-agentic", folder: "personal-agentic" },
    };
    const missingRef = {
      cap: "skill:a",
      source: { relHome: "~/team-agentic", folder: "team-agentic" },
    };
    useSuitesStore.setState({
      draft: {
        name: "Backend",
        description: "",
        capabilities: [missingRef, liveRef],
        agent: EMPTY_AGENT,
      },
    });

    useSuitesStore.getState().removeCapabilities([missingRef]);

    expect(useSuitesStore.getState().draft?.capabilities).toEqual([liveRef]);
  });

  it("cancelEdit while creating discards the draft", () => {
    useSuitesStore.setState({ isCreating: true, draft: { name: "x", description: "", capabilities: [], agent: EMPTY_AGENT } });

    useSuitesStore.getState().cancelEdit();
    const s = useSuitesStore.getState();

    expect(s.isCreating).toBe(false);
    expect(s.draft).toBeUndefined();
  });

  it("cancelEdit while editing reverts the draft to the selected suite", () => {
    useSuitesStore.setState({
      suites: [makeSuite({ name: "Backend", capabilities: [{ cap: "skill:a", source: SRC }] })],
      selectedId: "s1",
      draft: { name: "edited", description: "", capabilities: [], agent: EMPTY_AGENT },
    });

    useSuitesStore.getState().cancelEdit();

    expect(useSuitesStore.getState().draft).toEqual({
      name: "Backend",
      description: "",
      capabilities: [{ cap: "skill:a", source: SRC }],
      agent: EMPTY_AGENT,
    });
  });
});

describe("suites store — persistence", () => {
  it("save in create mode creates the suite, reloads, and selects it", async () => {
    const created = makeSuite({ id: "new", name: "Fresh", capabilities: [] });
    mocked.createSuite.mockResolvedValue(created);
    mocked.listSuites.mockResolvedValue([created]);
    useSuitesStore.setState({
      isCreating: true,
      draft: { name: "  Fresh  ", description: "  ", capabilities: [], agent: EMPTY_AGENT },
    });

    await useSuitesStore.getState().save();
    const s = useSuitesStore.getState();

    expect(mocked.createSuite).toHaveBeenCalledWith({
      name: "Fresh",
      description: null,
      capabilities: [],
      agent: null,
    });
    expect(s.selectedId).toBe("new");
    expect(s.isCreating).toBe(false);
  });

  it("save serializes a newly selected capability with its live source", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({
      suites: [makeSuite()],
      selectedId: "s1",
      draft: { name: "Backend", description: "core", capabilities: [], agent: EMPTY_AGENT },
    });
    useSuitesStore.getState().setCapabilities([makeItem("skill:a")], true);

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: "core",
      capabilities: [{ cap: "skill:a", source: SRC }],
      agent: null,
    });
  });

  it("save preserves a stale capability's original source qualification", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({
      suites: [makeSuite()],
      selectedId: "s1",
      draft: {
        name: "Backend",
        description: "core",
        capabilities: [{ cap: "skill:a", source: SRC }],
        agent: EMPTY_AGENT,
      },
    });

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: "core",
      capabilities: [{ cap: "skill:a", source: SRC }],
      agent: null,
    });
  });

  it("save persists an explicitly selected same-id item from another source", async () => {
    const missingSource: SourceRef = {
      relHome: "~/team-agentic",
      folder: "team-agentic",
    };
    const liveSource: SourceRef = {
      relHome: "~/personal-agentic",
      folder: "personal-agentic",
    };
    const suite = makeSuite({
      capabilities: [{ cap: "skill:a", source: missingSource }],
    });
    mocked.updateSuite.mockResolvedValue(suite);
    mocked.listSuites.mockResolvedValue([suite]);
    useSuitesStore.setState({
      suites: [suite],
    });
    useSuitesStore.getState().selectSuite("s1");
    const liveItem = makeItem("skill:a", liveSource);
    useSuitesStore.getState().setCapabilities([liveItem], true);
    useSuitesStore.getState().removeCapabilities([
      { cap: "skill:a", source: missingSource },
    ]);

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: null,
      capabilities: [{ cap: "skill:a", source: liveSource }],
      agent: null,
    });
  });

  it("save keeps a legacy stale capability unqualified", async () => {
    const legacy = makeSuite({ capabilities: [{ cap: "skill:a", source: null }] });
    mocked.updateSuite.mockResolvedValue(legacy);
    mocked.listSuites.mockResolvedValue([legacy]);
    useSuitesStore.setState({
      suites: [legacy],
      selectedId: "s1",
      draft: {
        name: "Backend",
        description: "",
        capabilities: [{ cap: "skill:a", source: null }],
        agent: EMPTY_AGENT,
      },
    });

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: null,
      capabilities: [{ cap: "skill:a", source: null }],
      agent: null,
    });
  });

  it("save is a no-op when the name is blank", async () => {
    useSuitesStore.setState({
      isCreating: true,
      draft: { name: "   ", description: "", capabilities: [], agent: EMPTY_AGENT },
    });

    await useSuitesStore.getState().save();

    expect(mocked.createSuite).not.toHaveBeenCalled();
  });

  it("remove deletes the selected suite and clears the selection", async () => {
    mocked.deleteSuite.mockResolvedValue(undefined);
    mocked.listSuites.mockResolvedValue([]);
    useSuitesStore.setState({ selectedId: "s1", draft: { name: "x", description: "", capabilities: [], agent: EMPTY_AGENT } });

    await useSuitesStore.getState().remove();
    const s = useSuitesStore.getState();

    expect(mocked.deleteSuite).toHaveBeenCalledWith("s1");
    expect(s.selectedId).toBeUndefined();
    expect(s.draft).toBeUndefined();
  });
});

describe("suites store — agent block", () => {
  const agentSuite = makeSuite({
    agent: { emoji: "🛠️", instructions: "Ship it.", requiredClis: ["gh"] },
  });

  it("selectSuite loads the agent block into the draft and fetches its version", async () => {
    useSuitesStore.setState({ suites: [agentSuite] });

    useSuitesStore.getState().selectSuite("s1");

    expect(useSuitesStore.getState().draft?.agent).toEqual({
      emoji: "🛠️",
      instructions: "Ship it.",
      requiredClis: ["gh"],
    });
    expect(mocked.agentVersion).toHaveBeenCalledWith("s1");
    await vi.waitFor(() =>
      expect(useSuitesStore.getState().agentVersion).toBe("abc123def456"),
    );
  });

  it("drops a version that resolves after the selection moved on", async () => {
    let resolveFirst: (v: string) => void = () => {};
    mocked.agentVersion
      .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r; }))
      .mockResolvedValueOnce("second000000");
    useSuitesStore.setState({ suites: [agentSuite, makeSuite({ id: "s2" })] });

    useSuitesStore.getState().selectSuite("s1");
    useSuitesStore.getState().selectSuite("s2");
    await vi.waitFor(() =>
      expect(useSuitesStore.getState().agentVersion).toBe("second000000"),
    );
    resolveFirst("first0000000");
    await Promise.resolve();

    expect(useSuitesStore.getState().agentVersion).toBe("second000000");
  });

  it("a failed version lookup leaves the version blank without a toast", async () => {
    mocked.agentVersion.mockRejectedValue(new Error("scan failed"));
    useSuitesStore.setState({ suites: [agentSuite] });

    useSuitesStore.getState().selectSuite("s1");
    await Promise.resolve();
    await Promise.resolve();

    expect(useSuitesStore.getState().agentVersion).toBeUndefined();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("startCreate clears the version and starts with an empty agent", () => {
    useSuitesStore.setState({ agentVersion: "abc123def456" });

    useSuitesStore.getState().startCreate();

    expect(useSuitesStore.getState().agentVersion).toBeUndefined();
    expect(useSuitesStore.getState().draft?.agent).toEqual(EMPTY_AGENT);
  });

  it("setAgent patches only the agent fields it is given", () => {
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");

    useSuitesStore.getState().setAgent({ emoji: "🧭" });

    expect(useSuitesStore.getState().draft?.agent).toEqual({
      emoji: "🧭",
      instructions: "Ship it.",
      requiredClis: ["gh"],
    });
  });

  it("toggleRequiredCli adds and removes catalog ids on the draft", () => {
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");

    useSuitesStore.getState().toggleRequiredCli("jq");
    expect(useSuitesStore.getState().draft?.agent.requiredClis).toEqual(["gh", "jq"]);

    useSuitesStore.getState().toggleRequiredCli("gh");
    expect(useSuitesStore.getState().draft?.agent.requiredClis).toEqual(["jq"]);
  });

  it("cancelEdit reverts agent edits to the stored block", () => {
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");
    useSuitesStore.getState().setAgent({ instructions: "changed" });

    useSuitesStore.getState().cancelEdit();

    expect(useSuitesStore.getState().draft?.agent.instructions).toBe("Ship it.");
  });

  it("save sends the trimmed agent block and refreshes the version", async () => {
    mocked.updateSuite.mockResolvedValue(agentSuite);
    mocked.listSuites.mockResolvedValue([agentSuite]);
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");
    useSuitesStore.getState().setAgent({ emoji: " 🧭 " });
    mocked.agentVersion.mockClear();

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: null,
      capabilities: [{ cap: "skill:a", source: SRC }],
      agent: { emoji: "🧭", instructions: "Ship it.", requiredClis: ["gh"] },
    });
    expect(mocked.agentVersion).toHaveBeenCalledWith("s1");
  });

  it("save clears the agent block when every field is emptied", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");
    useSuitesStore.getState().setAgent({ emoji: "", instructions: "", requiredClis: [] });

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite.mock.calls[0][1].agent).toBeNull();
  });

  it("save refuses an over-limit agent block with a toast and no IPC", async () => {
    useSuitesStore.setState({ suites: [agentSuite] });
    useSuitesStore.getState().selectSuite("s1");
    useSuitesStore.getState().setAgent({ instructions: "a".repeat(32 * 1024 + 1) });

    await useSuitesStore.getState().save();

    expect(mocked.updateSuite).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringMatching(/32 KiB/));
  });
});

describe("suites store — prune", () => {
  it("pruneSelection drops the selection when the selected suite has vanished", () => {
    useSuitesStore.setState({ suites: [makeSuite({ id: "other" })], selectedId: "s1" });

    useSuitesStore.getState().pruneSelection();

    expect(useSuitesStore.getState().selectedId).toBeUndefined();
  });

  it("setBase marks a suite as base via IPC and reloads", async () => {
    mocked.setBaseSuite.mockResolvedValue(undefined);
    mocked.listSuites.mockResolvedValue([makeSuite({ isBase: true })]);

    await useSuitesStore.getState().setBase("s1");

    expect(mocked.setBaseSuite).toHaveBeenCalledWith("s1");
    expect(mocked.listSuites).toHaveBeenCalled();
    expect(useSuitesStore.getState().suites[0].isBase).toBe(true);
  });

  it("setBase(null) clears the base suite via IPC", async () => {
    mocked.setBaseSuite.mockResolvedValue(undefined);
    mocked.listSuites.mockResolvedValue([makeSuite({ isBase: false })]);

    await useSuitesStore.getState().setBase(null);

    expect(mocked.setBaseSuite).toHaveBeenCalledWith(null);
  });
});
