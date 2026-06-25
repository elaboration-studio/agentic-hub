import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  applySuite: vi.fn(),
  suiteApplyPreview: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import { applySuite, suiteApplyPreview } from "@/ipc";
import { toast } from "sonner";
import type { ApplySuiteResult } from "@/types";
import { useApplyStore } from "./apply";

const mocked = {
  applySuite: vi.mocked(applySuite),
  suiteApplyPreview: vi.mocked(suiteApplyPreview),
};

function makeResult(overrides: Partial<ApplySuiteResult> = {}): ApplySuiteResult {
  return {
    applyResult: { created: 1, removed: 0, replaced: 0, refreshed: 0, skipped: 0, errors: [] },
    skippedStale: 0,
    skippedAbsentSource: 0,
    manualItemIds: [],
    suite: {
      id: "s1",
      name: "Suite",
      description: null,
      capabilities: [],
      isBase: false,
      createdAt: "t",
      updatedAt: "t",
    },
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useApplyStore.setState({ pending: null, busy: false });
});

describe("apply store — request", () => {
  it("applies directly when preview finds no extras", async () => {
    mocked.suiteApplyPreview.mockResolvedValue([]);
    mocked.applySuite.mockResolvedValue(makeResult());

    await useApplyStore.getState().request("cursor", "s1", "My Suite");

    expect(mocked.suiteApplyPreview).toHaveBeenCalledWith("cursor", "s1");
    expect(mocked.applySuite).toHaveBeenCalledWith("cursor", "s1", false);
    expect(useApplyStore.getState().pending).toBeNull();
    expect(toast.success).toHaveBeenCalled();
  });

  it("sets pending when preview finds extras", async () => {
    mocked.suiteApplyPreview.mockResolvedValue(["skill:extra"]);

    await useApplyStore.getState().request("cursor", "s1", "My Suite");

    expect(mocked.applySuite).not.toHaveBeenCalled();
    expect(useApplyStore.getState().pending).toEqual({
      tool: "cursor",
      suiteId: "s1",
      suiteName: "My Suite",
      extras: ["skill:extra"],
    });
  });
});

describe("apply store — confirm", () => {
  it("confirm(true) applies with preserve flag", async () => {
    mocked.applySuite.mockResolvedValue(makeResult());
    useApplyStore.setState({
      pending: { tool: "cursor", suiteId: "s1", suiteName: "My Suite", extras: ["skill:x"] },
    });

    await useApplyStore.getState().confirm(true);

    expect(mocked.applySuite).toHaveBeenCalledWith("cursor", "s1", true);
    expect(useApplyStore.getState().pending).toBeNull();
  });

  it("confirm(false) applies as full reset", async () => {
    mocked.applySuite.mockResolvedValue(makeResult());
    useApplyStore.setState({
      pending: { tool: "codex", suiteId: "s2", suiteName: "Other", extras: ["rule:a"] },
    });

    await useApplyStore.getState().confirm(false);

    expect(mocked.applySuite).toHaveBeenCalledWith("codex", "s2", false);
  });
});

describe("apply store — cancel", () => {
  it("clears pending without applying", () => {
    useApplyStore.setState({
      pending: { tool: "cursor", suiteId: "s1", suiteName: "X", extras: [] },
    });

    useApplyStore.getState().cancel();

    expect(useApplyStore.getState().pending).toBeNull();
    expect(mocked.applySuite).not.toHaveBeenCalled();
  });
});
