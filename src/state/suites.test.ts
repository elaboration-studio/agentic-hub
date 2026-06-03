import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listSuites: vi.fn(),
  createSuite: vi.fn(),
  updateSuite: vi.fn(),
  deleteSuite: vi.fn(),
  applySuite: vi.fn(),
  setBaseSuite: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  applySuite,
  createSuite,
  deleteSuite,
  listSuites,
  setBaseSuite,
  updateSuite,
} from "@/ipc";
import { toast } from "sonner";
import type {
  ApplyError,
  ApplySuiteResult,
  CapabilityItem,
  SourceRef,
  SuiteDefinition,
} from "@/types";
import { useSuitesStore } from "./suites";

const mocked = {
  listSuites: vi.mocked(listSuites),
  createSuite: vi.mocked(createSuite),
  updateSuite: vi.mocked(updateSuite),
  deleteSuite: vi.mocked(deleteSuite),
  applySuite: vi.mocked(applySuite),
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
  it("startCreate opens a blank draft and clears any selection", () => {
    useSuitesStore.setState({ selectedId: "s1" });

    useSuitesStore.getState().startCreate();
    const s = useSuitesStore.getState();

    expect(s.isCreating).toBe(true);
    expect(s.selectedId).toBeUndefined();
    expect(s.draft).toEqual({ name: "", description: "", capabilities: [] });
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

  it("save leaves a capability unqualified when its id is not in the scan", async () => {
    mocked.updateSuite.mockResolvedValue(makeSuite());
    mocked.listSuites.mockResolvedValue([makeSuite()]);
    useSuitesStore.setState({
      selectedId: "s1",
      draft: { name: "Backend", description: "core", capabilities: ["skill:a"] },
    });

    await useSuitesStore.getState().save([]);

    expect(mocked.updateSuite).toHaveBeenCalledWith("s1", {
      name: "Backend",
      description: "core",
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

describe("suites store — apply & prune", () => {
  it("applySelected toasts a success summary for a clean apply", async () => {
    const result: ApplySuiteResult = {
      applyResult: { created: 2, removed: 1, replaced: 0, refreshed: 0, skipped: 0, errors: [] },
      skippedStale: 0,
      skippedAbsentSource: 0,
      suite: makeSuite(),
    };
    mocked.applySuite.mockResolvedValue(result);
    useSuitesStore.setState({ selectedId: "s1" });

    await useSuitesStore.getState().applySelected("cursor");

    expect(mocked.applySuite).toHaveBeenCalledWith("cursor", "s1");
    expect(toast.success).toHaveBeenCalled();
    expect(toast.warning).not.toHaveBeenCalled();
  });

  it("applySelected warns when the apply reports errors", async () => {
    const result: ApplySuiteResult = {
      applyResult: {
        created: 0,
        removed: 0,
        replaced: 0,
        refreshed: 0,
        skipped: 0,
        errors: [{ message: "boom" } as ApplyError],
      },
      skippedStale: 0,
      skippedAbsentSource: 0,
      suite: makeSuite(),
    };
    mocked.applySuite.mockResolvedValue(result);
    useSuitesStore.setState({ selectedId: "s1" });

    await useSuitesStore.getState().applySelected("cursor");

    expect(toast.warning).toHaveBeenCalled();
  });

  it("applySelected surfaces capabilities preserved from absent sources", async () => {
    const result: ApplySuiteResult = {
      applyResult: { created: 1, removed: 0, replaced: 0, refreshed: 0, skipped: 0, errors: [] },
      skippedStale: 0,
      skippedAbsentSource: 2,
      suite: makeSuite(),
    };
    mocked.applySuite.mockResolvedValue(result);
    useSuitesStore.setState({ selectedId: "s1" });

    await useSuitesStore.getState().applySelected("cursor");

    const msg = vi.mocked(toast.success).mock.calls[0]?.[0] as string;
    expect(msg).toContain("2 from sources not on this machine, preserved");
  });

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
