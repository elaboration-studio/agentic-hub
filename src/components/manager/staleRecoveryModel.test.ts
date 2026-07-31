import { describe, expect, it } from "vitest";
import type { CapabilityKind, LinkState } from "@/types";
import { staleRecoveryActions } from "./staleRecoveryModel";

const KINDS: CapabilityKind[] = ["skill", "agent", "rule", "hook", "command"];

describe("stale recovery model", () => {
  it.each(KINDS)("offers staged refresh and manual fallbacks for an unowned stale %s", (kind) => {
    expect(staleRecoveryActions(kind, "stale", false)).toEqual([
      "refresh",
      "openSource",
      "revealTarget",
    ]);
  });

  it.each(KINDS)("offers suite re-sync and manual fallbacks for an owned stale %s", (kind) => {
    expect(staleRecoveryActions(kind, "stale", true)).toEqual([
      "resyncSuite",
      "openSource",
      "revealTarget",
    ]);
  });

  it.each<LinkState>(["enabled", "disabled", "broken", "foreign_file", "foreign_link"])(
    "offers no recovery actions for %s",
    (state) => {
      expect(staleRecoveryActions("skill", state, false)).toEqual([]);
      expect(staleRecoveryActions("skill", state, true)).toEqual([]);
    },
  );
});
