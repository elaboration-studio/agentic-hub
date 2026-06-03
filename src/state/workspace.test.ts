import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listWorkspaceTargets: vi.fn(),
  listSuites: vi.fn(),
  pickWorkspaceDir: vi.fn(),
  removeWorkspaceTarget: vi.fn(),
  setActiveWorkspaceTarget: vi.fn(),
  applyWorkspacePatch: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  applyWorkspacePatch,
  listSuites,
  listWorkspaceTargets,
  pickWorkspaceDir,
  removeWorkspaceTarget,
  setActiveWorkspaceTarget,
} from "@/ipc";
import { toast } from "sonner";
import type {
  SuiteDefinition,
  WorkspacePatchResult,
  WorkspaceTarget,
} from "@/types";
import { useWorkspaceStore } from "./workspace";

const mocked = {
  listWorkspaceTargets: vi.mocked(listWorkspaceTargets),
  listSuites: vi.mocked(listSuites),
  pickWorkspaceDir: vi.mocked(pickWorkspaceDir),
  removeWorkspaceTarget: vi.mocked(removeWorkspaceTarget),
  setActiveWorkspaceTarget: vi.mocked(setActiveWorkspaceTarget),
  applyWorkspacePatch: vi.mocked(applyWorkspacePatch),
};

function makeTarget(id: string): WorkspaceTarget {
  return { id, label: id, dir: `/work/${id}`, lastUsedAt: "2026-01-01", lastApplied: [] };
}

function makeSuite(id: string): SuiteDefinition {
  return {
    id,
    name: id,
    description: null,
    capabilities: [],
    isBase: false,
    createdAt: "2026-01-01",
    updatedAt: "2026-01-01",
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useWorkspaceStore.setState(useWorkspaceStore.getInitialState(), true);
});

describe("workspace store — reload", () => {
  it("merges targets and suites and honors the persisted active id", async () => {
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("a"), makeTarget("b")],
      workspaceActiveId: "b",
    });
    mocked.listSuites.mockResolvedValue([makeSuite("s1")]);

    await useWorkspaceStore.getState().reload();
    const s = useWorkspaceStore.getState();

    expect(s.targets).toHaveLength(2);
    expect(s.activeId).toBe("b");
    expect(s.suiteId).toBe("s1");
  });

  it("falls back to the first target when no active id is persisted", async () => {
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("a"), makeTarget("b")],
      workspaceActiveId: null,
    });
    mocked.listSuites.mockResolvedValue([]);

    await useWorkspaceStore.getState().reload();

    expect(useWorkspaceStore.getState().activeId).toBe("a");
  });

  it("keeps the current suite selection when it still exists", async () => {
    mocked.listWorkspaceTargets.mockResolvedValue({ workspaceTargets: [], workspaceActiveId: null });
    mocked.listSuites.mockResolvedValue([makeSuite("s1"), makeSuite("s2")]);
    useWorkspaceStore.setState({ suiteId: "s2" });

    await useWorkspaceStore.getState().reload();

    expect(useWorkspaceStore.getState().suiteId).toBe("s2");
  });

  it("toasts an error when a load step throws", async () => {
    mocked.listWorkspaceTargets.mockRejectedValue(new Error("no state"));
    mocked.listSuites.mockResolvedValue([]);

    await useWorkspaceStore.getState().reload();

    expect(toast.error).toHaveBeenCalledWith("no state");
  });
});

describe("workspace store — activation", () => {
  it("activate sets the active id and persists it through IPC", async () => {
    mocked.setActiveWorkspaceTarget.mockResolvedValue(undefined);

    await useWorkspaceStore.getState().activate("b");

    expect(useWorkspaceStore.getState().activeId).toBe("b");
    expect(mocked.setActiveWorkspaceTarget).toHaveBeenCalledWith("b");
  });
});

describe("workspace store — apply", () => {
  it("apply is a no-op without an active target", async () => {
    useWorkspaceStore.setState({ activeId: "", suiteId: "s1" });

    await useWorkspaceStore.getState().apply();

    expect(mocked.applyWorkspacePatch).not.toHaveBeenCalled();
  });

  it("apply is a no-op without a selected suite", async () => {
    useWorkspaceStore.setState({ activeId: "a", suiteId: "" });

    await useWorkspaceStore.getState().apply();

    expect(mocked.applyWorkspacePatch).not.toHaveBeenCalled();
  });

  it("apply patches the active target with the chosen tool and suite, storing the result", async () => {
    const result: WorkspacePatchResult = {
      tool: "cursor",
      workspaceDir: "/work/a",
      suiteId: "s1",
      suiteName: "s1",
      applied: ["x"],
      removed: [],
      skippedStaleIds: [],
      notes: [],
      errors: [],
    };
    mocked.applyWorkspacePatch.mockResolvedValue(result);
    useWorkspaceStore.setState({ activeId: "a", suiteId: "s1", tool: "cursor" });

    await useWorkspaceStore.getState().apply();

    expect(mocked.applyWorkspacePatch).toHaveBeenCalledWith("a", "cursor", "s1");
    expect(useWorkspaceStore.getState().result).toEqual(result);
  });
});

describe("workspace store — setters", () => {
  it("setSuiteId and setTool update their fields", () => {
    useWorkspaceStore.getState().setSuiteId("s9");
    useWorkspaceStore.getState().setTool("claude");
    const s = useWorkspaceStore.getState();

    expect(s.suiteId).toBe("s9");
    expect(s.tool).toBe("claude");
  });
});
