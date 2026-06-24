import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  checkForUpdate: vi.fn(),
  installUpdate: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import { checkForUpdate, installUpdate, type AvailableUpdate } from "@/ipc";
import { toast } from "sonner";
import { UPDATE_CHECK_INTERVAL_MS, useUpdateStore } from "./update";

const mocked = {
  checkForUpdate: vi.mocked(checkForUpdate),
  installUpdate: vi.mocked(installUpdate),
};

function makeUpdate(over: Partial<AvailableUpdate> = {}): AvailableUpdate {
  return {
    version: "0.9.3",
    notes: "Bug fixes",
    // The handle is opaque; installUpdate is mocked, so a stub is enough.
    handle: {} as AvailableUpdate["handle"],
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useUpdateStore.setState(useUpdateStore.getInitialState(), true);
});

describe("update store — check", () => {
  it("surfaces an available update without toasting", async () => {
    mocked.checkForUpdate.mockResolvedValue(makeUpdate());

    await useUpdateStore.getState().check();

    expect(useUpdateStore.getState().phase).toBe("available");
    expect(useUpdateStore.getState().available?.version).toBe("0.9.3");
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("toasts up-to-date on a non-silent check with no update", async () => {
    mocked.checkForUpdate.mockResolvedValue(null);

    await useUpdateStore.getState().check();

    expect(useUpdateStore.getState().phase).toBe("uptodate");
    expect(toast.success).toHaveBeenCalledTimes(1);
  });

  it("stays quiet on a silent check with no update", async () => {
    mocked.checkForUpdate.mockResolvedValue(null);

    await useUpdateStore.getState().check({ silent: true });

    expect(useUpdateStore.getState().phase).toBe("uptodate");
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("records the error and toasts when the check throws", async () => {
    mocked.checkForUpdate.mockRejectedValue(new Error("network down"));

    await useUpdateStore.getState().check();

    expect(useUpdateStore.getState().phase).toBe("error");
    expect(useUpdateStore.getState().error).toBe("network down");
    expect(toast.error).toHaveBeenCalledWith("network down");
  });

  it("stamps lastCheckedAt so the throttle has a baseline", async () => {
    mocked.checkForUpdate.mockResolvedValue(null);

    await useUpdateStore.getState().check({ silent: true });

    expect(useUpdateStore.getState().lastCheckedAt).not.toBeNull();
  });
});

describe("update store — maybeCheck (weekly throttle)", () => {
  it("checks when no check has ever run", async () => {
    mocked.checkForUpdate.mockResolvedValue(null);

    await useUpdateStore.getState().maybeCheck(1_000_000);

    expect(mocked.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("skips when the last check is within the weekly window", async () => {
    const now = 1_000_000_000;
    useUpdateStore.setState({ lastCheckedAt: now - 1000 });

    await useUpdateStore.getState().maybeCheck(now);

    expect(mocked.checkForUpdate).not.toHaveBeenCalled();
  });

  it("checks again once the weekly window has elapsed", async () => {
    const now = 1_000_000_000;
    useUpdateStore.setState({ lastCheckedAt: now - UPDATE_CHECK_INTERVAL_MS - 1 });
    mocked.checkForUpdate.mockResolvedValue(null);

    await useUpdateStore.getState().maybeCheck(now);

    expect(mocked.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("does not run while a check or download is already in flight", async () => {
    useUpdateStore.setState({ phase: "downloading", lastCheckedAt: null });

    await useUpdateStore.getState().maybeCheck(1_000_000);

    expect(mocked.checkForUpdate).not.toHaveBeenCalled();
  });
});

describe("update store — install", () => {
  it("installs the pending update via the handle", async () => {
    const update = makeUpdate();
    useUpdateStore.setState({ phase: "available", available: update });
    mocked.installUpdate.mockResolvedValue(undefined);

    await useUpdateStore.getState().install();

    expect(mocked.installUpdate).toHaveBeenCalledWith(update);
    expect(useUpdateStore.getState().phase).toBe("downloading");
  });

  it("is a no-op when there is no available update", async () => {
    await useUpdateStore.getState().install();

    expect(mocked.installUpdate).not.toHaveBeenCalled();
  });

  it("records the error and toasts when install fails", async () => {
    useUpdateStore.setState({ phase: "available", available: makeUpdate() });
    mocked.installUpdate.mockRejectedValue(new Error("disk full"));

    await useUpdateStore.getState().install();

    expect(useUpdateStore.getState().phase).toBe("error");
    expect(toast.error).toHaveBeenCalledWith("disk full");
  });
});

describe("update store — dismiss", () => {
  it("clears the available banner", () => {
    useUpdateStore.setState({ phase: "available", available: makeUpdate() });

    useUpdateStore.getState().dismiss();

    expect(useUpdateStore.getState().phase).toBe("idle");
    expect(useUpdateStore.getState().available).toBeNull();
  });
});
