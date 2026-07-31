import { describe, expect, it } from "vitest";

import { resolveAppRoute } from "./navigation";
import { enterSuiteManagerScope } from "./state/scopeNavigation";

describe("app navigation aliases", () => {
  it("opens the legacy suites hash as Manager in suite scope", () => {
    expect(resolveAppRoute("#/suites")).toEqual({ route: "manager", managerScope: "suite" });
  });

  it("loads global manager data before applying the suites alias from Workspace", async () => {
    const events: string[] = [];
    const resolved = resolveAppRoute("#/suites");
    const state = {
      data: { id: "workspace" } as object | null,
      readOnly: true,
      refresh: async () => {
        events.push("global-loaded");
        state.data = { id: "global" };
        state.readOnly = false;
      },
      setScope: (scope: "global" | "suite" | "workspace") => {
        events.push(`scope:${scope}`);
      },
    };

    if (resolved.managerScope === "suite") {
      await enterSuiteManagerScope(() => state);
    }

    expect(events).toEqual(["global-loaded", "scope:suite"]);
  });

  it("keeps normal routes unchanged", () => {
    expect(resolveAppRoute("#/statistics")).toEqual({ route: "statistics" });
  });
});
