import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listSuites: vi.fn(),
  createSuite: vi.fn(),
  updateSuite: vi.fn(),
  deleteSuite: vi.fn(),
  setBaseSuite: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
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
import { suiteRefMatchesItem, useSuitesStore } from "./suites";

const mocked = {
  listSuites: vi.mocked(listSuites),
  createSuite: vi.mocked(createSuite),
  updateSuite: vi.mocked(updateSuite),
  deleteSuite: vi.mocked(deleteSuite),
  setBaseSuite: vi.mocked(setBaseSuite),
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

  it("startCreate opens a blank draft and clears any selection", () => {
    useSuitesStore.setState({ selectedId: "s1" });

    useSuitesStore.getState().startCreate();
    const s = useSuitesStore.getState();

    expect(s.isCreating).toBe(true);
    expect(s.selectedId).toBeUndefined();
    expect(s.draft).toEqual({ name: "", description: "", capabilities: [] });
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
      capabilities: ["skill:enabled"],
    });
  });

  it("selectSuite loads the chosen suite into the draft, normalizing null description", () => {
    useSuitesStore.setState({ suites: [makeSuite({ description: null })] });

    useSuitesStore.getState().selectSuite("s1");
    const s = useSuitesStore.getState();

    expect(s.selectedId).toBe("s1");
    expect(s.draft).toEqual({ name: "Backend", description: "", capabilities: ["skill:a"] });
  });

  it("selectSuite is a no-op for an unknown id", () => {
    useSuitesStore.setState({ suites: [makeSuite()] });

    useSuitesStore.getState().selectSuite("missing");

    expect(useSuitesStore.getState().selectedId).toBeUndefined();
  });

  it("setCapabilities adds ids without duplicating, and removes when off", () => {
    useSuitesStore.setState({ draft: { name: "x", description: "", capabilities: ["skill:a"] } });

    useSuitesStore.getState().setCapabilities(["skill:a", "skill:b"], true);
    expect(useSuitesStore.getState().draft?.capabilities).toEqual(["skill:a", "skill:b"]);

    useSuitesStore.getState().setCapabilities(["skill:a"], false);
    expect(useSuitesStore.getState().draft?.capabilities).toEqual(["skill:b"]);
  });

  it("removeCapabilities drops only the requested stale references", () => {
    useSuitesStore.setState({
      draft: {
        name: "Backend",
        description: "",
        capabilities: ["skill:live", "skill:missing"],
      },
    });

    useSuitesStore.getState().removeCapabilities(["skill:missing"]);

    expect(useSuitesStore.getState().draft?.capabilities).toEqual(["skill:live"]);
  });

  it("cancelEdit while creating discards the draft", () => {
    useSuitesStore.setState({ isCreating: true, draft: { name: "x", description: "", capabilities: [] } });

    useSuitesStore.getState().cancelEdit();
    const s = useSuitesStore.getState();

    expect(s.isCreating).toBe(false);
    expect(s.draft).toBeUndefined();
  });

  it("cancelEdit while editing reverts the draft to the selected suite", () => {
    useSuitesStore.setState({
      suites: [makeSuite({ name: "Backend", capabilities: [{ cap: "skill:a", source: SRC }] })],
      selectedId: "s1",
      draft: { name: "edited", description: "", capabilities: [] },
    });

    useSuitesStore.getState().cancelEdit();

    expect(useSuitesStore.getState().draft).toEqual({
      name: "Backend",
      description: "",
      capabilities: ["skill:a"],
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
      draft: { name: "  Fresh  ", description: "  ", capabilities: [] },
    });

    await useSuitesStore.getState().save([]);
    const s = useSuitesStore.getState();

    expect(mocked.createSuite).toHaveBeenCalledWith({
      name: "Fresh",
      description: null,
      capabilities: [],
    });
    expect(s.selectedId).toBe("new");
    expect(s.isCreating).toBe(false);
  });

  it("save attaches each capability's source from the live scan", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({
      suites: [makeSuite()],
      selectedId: "s1",
      draft: { name: "Backend", description: "core", capabilities: ["skill:a"] },
    });

    await useSuitesStore.getState().save([makeItem("skill:a")]);

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: "core",
      capabilities: [{ cap: "skill:a", source: SRC }],
    });
  });

  it("save preserves a stale capability's original source qualification", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({
      suites: [makeSuite()],
      selectedId: "s1",
      draft: { name: "Backend", description: "core", capabilities: ["skill:a"] },
    });

    await useSuitesStore.getState().save([]);

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: "core",
      capabilities: [{ cap: "skill:a", source: SRC }],
    });
  });

  it("save keeps a legacy stale capability unqualified", async () => {
    const legacy = makeSuite({ capabilities: [{ cap: "skill:a", source: null }] });
    mocked.updateSuite.mockResolvedValue(legacy);
    mocked.listSuites.mockResolvedValue([legacy]);
    useSuitesStore.setState({
      suites: [legacy],
      selectedId: "s1",
      draft: { name: "Backend", description: "", capabilities: ["skill:a"] },
    });

    await useSuitesStore.getState().save([]);

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: null,
      capabilities: [{ cap: "skill:a", source: null }],
    });
  });

  it("save is a no-op when the name is blank", async () => {
    useSuitesStore.setState({
      isCreating: true,
      draft: { name: "   ", description: "", capabilities: [] },
    });

    await useSuitesStore.getState().save([]);

    expect(mocked.createSuite).not.toHaveBeenCalled();
  });

  it("remove deletes the selected suite and clears the selection", async () => {
    mocked.deleteSuite.mockResolvedValue(undefined);
    mocked.listSuites.mockResolvedValue([]);
    useSuitesStore.setState({ selectedId: "s1", draft: { name: "x", description: "", capabilities: [] } });

    await useSuitesStore.getState().remove();
    const s = useSuitesStore.getState();

    expect(mocked.deleteSuite).toHaveBeenCalledWith("s1");
    expect(s.selectedId).toBeUndefined();
    expect(s.draft).toBeUndefined();
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
