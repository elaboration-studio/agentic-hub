import type { CapabilityKind, LinkState } from "@/types";

export type StaleRecoveryAction =
  | "refresh"
  | "resyncSuite"
  | "openSource"
  | "revealTarget";

export function staleRecoveryActions(
  _kind: CapabilityKind,
  state: LinkState,
  suiteOwned: boolean,
): StaleRecoveryAction[] {
  if (state !== "stale") return [];
  return [
    suiteOwned ? "resyncSuite" : "refresh",
    "openSource",
    "revealTarget",
  ];
}
