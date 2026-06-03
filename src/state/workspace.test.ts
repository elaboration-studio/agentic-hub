import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listWorkspaceTargets: vi.fn(),
  pickWorkspaceDir: vi.fn(),
  removeWorkspaceTarget: vi.fn(),
  setActiveWorkspaceTarget: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

// The workspace store hands the active id off to the manager store's
// loadWorkspace; mock that boundary so we can assert the handoff in isolation.
const loadWorkspace = vi.fn();
vi.mock("./manager", () => ({
  useManagerStore: { getState: () => ({ loadWorkspace }) },
}));

import {
  listWorkspaceTargets,
  pickWorkspaceDir,
  removeWorkspaceTarget,
  setActiveWorkspaceTarget,
} from "@/ipc";
import { toast } from "sonner";
import type { WorkspaceTarget } from "@/types";
import { useWorkspaceStore } from "./workspace";

const mocked = {
  listWorkspaceTargets: vi.mocked(listWorkspaceTargets),
  pickWorkspaceDir: vi.mocked(pickWorkspaceDir),
  removeWorkspaceTarget: vi.mocked(removeWorkspaceTarget),
  setActiveWorkspaceTarget: vi.mocked(setActiveWorkspaceTarget),
};

function makeTarget(id: string): WorkspaceTarget {
  return { id, label: id, dir: `/work/${id}`, lastUsedAt: "2026-01-01" };
}

beforeEach(() => {
  vi.clearAllMocks();
  useWorkspaceStore.setState(useWorkspaceStore.getInitialState(), true);
});

describe("workspace store — reload", () => {
  it("loads targets and honors the persisted active id", async () => {
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("a"), makeTarget("b")],
      workspaceActiveId: "b",
    });

    await useWorkspaceStore.getState().reload();
    const s = useWorkspaceStore.getState();

    expect(s.targets).toHaveLength(2);
    expect(s.activeId).toBe("b");
  });

  it("falls back to the first target when no active id is persisted", async () => {
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("a"), makeTarget("b")],
      workspaceActiveId: null,
    });

    await useWorkspaceStore.getState().reload();

    expect(useWorkspaceStore.getState().activeId).toBe("a");
  });

  it("toasts an error when listing targets throws", async () => {
    mocked.listWorkspaceTargets.mockRejectedValue(new Error("no state"));

    await useWorkspaceStore.getState().reload();

    expect(toast.error).toHaveBeenCalledWith("no state");
  });
});

describe("workspace store — activation", () => {
  it("activate sets the active id, persists it, and loads the inventory", async () => {
    mocked.setActiveWorkspaceTarget.mockResolvedValue(undefined);

    await useWorkspaceStore.getState().activate("b");

    expect(useWorkspaceStore.getState().activeId).toBe("b");
    expect(mocked.setActiveWorkspaceTarget).toHaveBeenCalledWith("b");
    expect(loadWorkspace).toHaveBeenCalledWith("b");
  });
});

describe("workspace store — pick", () => {
  it("adds a folder, makes it active, and loads its inventory", async () => {
    mocked.pickWorkspaceDir.mockResolvedValue(makeTarget("c"));
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("c")],
      workspaceActiveId: "c",
    });

    await useWorkspaceStore.getState().pick();

    expect(useWorkspaceStore.getState().activeId).toBe("c");
    expect(loadWorkspace).toHaveBeenCalledWith("c");
  });

  it("stays silent when the folder dialog is cancelled", async () => {
    mocked.pickWorkspaceDir.mockRejectedValue(new Error("No folder selected"));

    await useWorkspaceStore.getState().pick();

    expect(toast.error).not.toHaveBeenCalled();
    expect(loadWorkspace).not.toHaveBeenCalled();
  });
});

describe("workspace store — remove", () => {
  it("removes a target, reloads, and loads the next active inventory", async () => {
    mocked.removeWorkspaceTarget.mockResolvedValue(undefined);
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [makeTarget("a")],
      workspaceActiveId: "a",
    });

    await useWorkspaceStore.getState().remove("b");

    expect(mocked.removeWorkspaceTarget).toHaveBeenCalledWith("b");
    expect(useWorkspaceStore.getState().activeId).toBe("a");
    expect(loadWorkspace).toHaveBeenCalledWith("a");
  });

  it("loads nothing when the last workspace is removed", async () => {
    mocked.removeWorkspaceTarget.mockResolvedValue(undefined);
    mocked.listWorkspaceTargets.mockResolvedValue({
      workspaceTargets: [],
      workspaceActiveId: null,
    });

    await useWorkspaceStore.getState().remove("a");

    expect(useWorkspaceStore.getState().activeId).toBe("");
    expect(loadWorkspace).not.toHaveBeenCalled();
  });
});
