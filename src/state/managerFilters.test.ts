import { beforeEach, describe, expect, it } from "vitest";

import { useManagerFiltersStore } from "./managerFilters";

beforeEach(() => {
  useManagerFiltersStore.setState(useManagerFiltersStore.getInitialState(), true);
});

describe("managerFilters store", () => {
  it("defaults to tree view with empty filters", () => {
    const s = useManagerFiltersStore.getState();
    expect(s.view).toBe("tree");
    expect(s.query).toBe("");
    expect(s.source).toBe("");
    expect(s.kind).toBe("all");
    expect(s.enabledOnly).toBe(false);
    expect(s.collapsed.size).toBe(0);
  });

  it("setters update each filter field", () => {
    const { setView, setQuery, setSource, setKind, setEnabledOnly } =
      useManagerFiltersStore.getState();
    setView("flat");
    setQuery("auth");
    setSource("shared");
    setKind("skill");
    setEnabledOnly(true);

    const s = useManagerFiltersStore.getState();
    expect(s.view).toBe("flat");
    expect(s.query).toBe("auth");
    expect(s.source).toBe("shared");
    expect(s.kind).toBe("skill");
    expect(s.enabledOnly).toBe(true);
  });

  it("setCollapsed replaces the collapsed set", () => {
    useManagerFiltersStore.getState().setCollapsed(new Set(["a", "b"]));
    expect([...useManagerFiltersStore.getState().collapsed]).toEqual(["a", "b"]);
  });

  it("toggleCollapsed adds a folder path when absent", () => {
    useManagerFiltersStore.getState().toggleCollapsed("advisors/people");
    expect(useManagerFiltersStore.getState().collapsed.has("advisors/people")).toBe(true);
  });

  it("toggleCollapsed removes a folder path when present", () => {
    const { toggleCollapsed } = useManagerFiltersStore.getState();
    toggleCollapsed("advisors/people");
    toggleCollapsed("advisors/people");
    expect(useManagerFiltersStore.getState().collapsed.has("advisors/people")).toBe(false);
  });

  it("toggleCollapsed produces a new Set instance for identity-based rerenders", () => {
    const before = useManagerFiltersStore.getState().collapsed;
    useManagerFiltersStore.getState().toggleCollapsed("x");
    expect(useManagerFiltersStore.getState().collapsed).not.toBe(before);
  });
});
