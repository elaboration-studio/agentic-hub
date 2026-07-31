import { describe, expect, it, vi } from "vitest";

import {
  addWorkspaceAndEnterScope,
  enterSuiteManagerScope,
} from "./scopeNavigation";

describe("manager scope navigation", () => {
  it("loads global data before entering suite scope from a workspace", async () => {
    const events: string[] = [];
    const setScope = vi.fn((scope: "global" | "suite" | "workspace") => {
      events.push(`scope:${scope}`);
    });
    const state = {
      data: { id: "workspace" } as object | null,
      readOnly: true,
      refresh: vi.fn(async () => {
        events.push("refresh");
        state.data = { id: "global" };
        state.readOnly = false;
      }),
      setScope,
    };

    const entered = await enterSuiteManagerScope(() => state);

    expect(entered).toBe(true);
    expect(events).toEqual(["refresh", "scope:suite"]);
  });

  it("does not enter suite scope when global data cannot be loaded", async () => {
    const setScope = vi.fn();
    const state = {
      data: null,
      readOnly: true,
      refresh: vi.fn(async () => undefined),
      setScope,
    };

    const entered = await enterSuiteManagerScope(() => state);

    expect(entered).toBe(false);
    expect(setScope).not.toHaveBeenCalled();
  });

  it("keeps the current scope when the first workspace picker is cancelled", async () => {
    const setScope = vi.fn();

    const entered = await addWorkspaceAndEnterScope(
      vi.fn(async () => false),
      setScope,
    );

    expect(entered).toBe(false);
    expect(setScope).not.toHaveBeenCalled();
  });

  it("enters workspace scope after the picker succeeds", async () => {
    const setScope = vi.fn();

    const entered = await addWorkspaceAndEnterScope(
      vi.fn(async () => true),
      setScope,
    );

    expect(entered).toBe(true);
    expect(setScope).toHaveBeenCalledWith("workspace");
  });
});
