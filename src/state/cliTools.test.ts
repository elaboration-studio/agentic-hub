import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listToolCatalog: vi.fn(),
  checkTool: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import { checkTool, listToolCatalog } from "@/ipc";
import { toast } from "sonner";
import type { CliTool, CliToolStatus } from "@/types";
import { useCliToolsStore } from "./cliTools";

const mocked = {
  listToolCatalog: vi.mocked(listToolCatalog),
  checkTool: vi.mocked(checkTool),
};

function makeTool(id: string): CliTool {
  return {
    id,
    name: id,
    category: null,
    installUrl: `https://example.com/${id}`,
    check: { program: id, args: ["--version"] },
    auth: null,
  };
}

function makeStatus(id: string, over: Partial<CliToolStatus> = {}): CliToolStatus {
  return {
    id,
    installed: true,
    version: "1.0.0",
    auth: "notApplicable",
    message: null,
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useCliToolsStore.setState(useCliToolsStore.getInitialState(), true);
});

describe("cliTools store — loadCatalog", () => {
  it("loads the catalog and flips loaded", async () => {
    mocked.listToolCatalog.mockResolvedValue([makeTool("node"), makeTool("gh")]);

    await useCliToolsStore.getState().loadCatalog();

    expect(useCliToolsStore.getState().catalog).toHaveLength(2);
    expect(useCliToolsStore.getState().loaded).toBe(true);
    expect(useCliToolsStore.getState().loading).toBe(false);
  });

  it("toasts and clears loading when the catalog load throws", async () => {
    mocked.listToolCatalog.mockRejectedValue(new Error("boom"));

    await useCliToolsStore.getState().loadCatalog();

    expect(toast.error).toHaveBeenCalledWith("boom");
    expect(useCliToolsStore.getState().loaded).toBe(false);
    expect(useCliToolsStore.getState().loading).toBe(false);
  });
});

describe("cliTools store — checkOne", () => {
  it("stores the status by id and clears the checking flag", async () => {
    mocked.checkTool.mockResolvedValue(makeStatus("gh", { auth: "authed" }));

    await useCliToolsStore.getState().checkOne("gh");

    expect(useCliToolsStore.getState().statusById.gh.auth).toBe("authed");
    expect(useCliToolsStore.getState().checking.has("gh")).toBe(false);
  });

  it("toasts and clears the checking flag when a probe throws", async () => {
    mocked.checkTool.mockRejectedValue(new Error("probe failed"));

    await useCliToolsStore.getState().checkOne("gh");

    expect(toast.error).toHaveBeenCalledWith("probe failed");
    expect(useCliToolsStore.getState().checking.has("gh")).toBe(false);
    expect(useCliToolsStore.getState().statusById.gh).toBeUndefined();
  });
});

describe("cliTools store — checkAll", () => {
  it("probes every catalog tool and records each status", async () => {
    useCliToolsStore.setState({ catalog: [makeTool("node"), makeTool("gh")] });
    mocked.checkTool.mockImplementation(async (id: string) =>
      makeStatus(id, { installed: id === "node" }),
    );

    await useCliToolsStore.getState().checkAll();

    expect(mocked.checkTool).toHaveBeenCalledTimes(2);
    expect(useCliToolsStore.getState().statusById.node.installed).toBe(true);
    expect(useCliToolsStore.getState().statusById.gh.installed).toBe(false);
    expect(useCliToolsStore.getState().checking.size).toBe(0);
  });
});
