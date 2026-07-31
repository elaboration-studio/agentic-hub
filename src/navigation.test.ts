import { describe, expect, it } from "vitest";

import { resolveAppRoute } from "./navigation";

describe("app navigation aliases", () => {
  it("opens the legacy suites hash as Manager in suite scope", () => {
    expect(resolveAppRoute("#/suites")).toEqual({ route: "manager", managerScope: "suite" });
  });

  it("keeps normal routes unchanged", () => {
    expect(resolveAppRoute("#/statistics")).toEqual({ route: "statistics" });
  });
});
