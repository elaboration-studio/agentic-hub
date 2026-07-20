import type { CapabilityItem, UsageStats, UsageTopRow } from "../types";
import type { UsageSort } from "../shared";

function compareLastUsed(ta: string | null | undefined, tb: string | null | undefined): number {
  const a = ta ?? "";
  const b = tb ?? "";
  if (a === b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return b.localeCompare(a);
}

function compareUsageCount(ca: number, cb: number): number {
  if (ca === cb) return 0;
  return cb - ca;
}

/** Compare two matrix rows for the selected usage sort. Ties break on name. */
export function compareByUsageSort(
  a: CapabilityItem,
  b: CapabilityItem,
  sort: UsageSort,
  stats: ReadonlyMap<string, UsageStats>,
): number {
  const sa = stats.get(a.id);
  const sb = stats.get(b.id);

  if (sort === "lastUsed") {
    const byLastUsed = compareLastUsed(sa?.lastUsedAt, sb?.lastUsedAt);
    if (byLastUsed !== 0) return byLastUsed;
  } else {
    const byCount = compareUsageCount(sa?.executionCount ?? 0, sb?.executionCount ?? 0);
    if (byCount !== 0) return byCount;
  }

  return a.name.localeCompare(b.name);
}

/** Compare Statistics usage table rows. Ties break on name. */
export function compareUsageTopRows(a: UsageTopRow, b: UsageTopRow, sort: UsageSort): number {
  if (sort === "lastUsed") {
    const byLastUsed = compareLastUsed(a.lastUsedAt, b.lastUsedAt);
    if (byLastUsed !== 0) return byLastUsed;
  } else {
    const byCount = compareUsageCount(a.executionCount, b.executionCount);
    if (byCount !== 0) return byCount;
  }

  return a.name.localeCompare(b.name);
}
