import { describe, expect, it } from "vitest";
import {
  getDefaultResourcePane,
  resolveInitialLoadedResourcePane,
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

  it("uses the enabled Skills default when availability first loads", () => {
    expect(resolveInitialLoadedResourcePane("tools", true, false)).toBe("skills");
  });

  it("does not override a pane selected before availability first loads", () => {
    expect(resolveInitialLoadedResourcePane("sessions", true, true)).toBe(
      "sessions",
    );
  });

  it("still falls back when an explicitly selected Skills pane is disabled", () => {
    expect(resolveInitialLoadedResourcePane("skills", false, true)).toBe("tools");
  });

  it("keeps available selections active", () => {
    expect(resolveResourcePane("skills", true)).toBe("skills");
    expect(resolveResourcePane("tools", false)).toBe("tools");
    expect(resolveResourcePane("sessions", false)).toBe("sessions");
  });
});
