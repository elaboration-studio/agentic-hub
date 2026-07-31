import { describe, expect, it } from "vitest";
import {
  getDefaultResourcePane,
  resolveResourcePane,
  RESOURCE_PANES,
} from "./resourcePanes";

describe("resource panes", () => {
  it("orders the Resources rail by expected usage frequency", () => {
    expect(RESOURCE_PANES).toEqual(["skills", "tools", "sessions"]);
  });

  it("defaults to Skills only when skills.sh is enabled", () => {
    expect(getDefaultResourcePane(true)).toBe("skills");
    expect(getDefaultResourcePane(false)).toBe("tools");
  });

  it("falls back to Tools when Skills becomes unavailable", () => {
    expect(resolveResourcePane("skills", false)).toBe("tools");
  });

  it("keeps available selections active", () => {
    expect(resolveResourcePane("skills", true)).toBe("skills");
    expect(resolveResourcePane("tools", false)).toBe("tools");
    expect(resolveResourcePane("sessions", false)).toBe("sessions");
  });
});
