import { describe, expect, it } from "vitest";

import type { CapabilityItem, UsageStats, UsageTopRow } from "../types";
import { compareByUsageSort, compareUsageTopRows } from "./managerSort";

function item(id: string, name: string): CapabilityItem {
  return {
    id,
    name,
    kind: "skill",
    sourceId: "shared",
    sourceLabel: "Shared",
    sourcePath: `/shared/${name}`,
    relativePath: name,
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

function stats(
  capabilityId: string,
  executionCount: number,
  lastUsedAt: string | null,
): UsageStats {
  return {
    capabilityId,
    executionCount,
    successCount: executionCount,
    failureCount: 0,
    lastUsedAt,
    toolBuckets: [],
  };
}

describe("compareByUsageSort", () => {
  const map = new Map<string, UsageStats>([
    ["a", stats("a", 10, "2026-07-20T12:00:00Z")],
    ["b", stats("b", 5, "2026-07-19T12:00:00Z")],
    ["c", stats("c", 0, null)],
  ]);

  it("sorts by latest use descending with never-used rows last", () => {
    const rows = [item("c", "c"), item("b", "b"), item("a", "a")];
    rows.sort((x, y) => compareByUsageSort(x, y, "lastUsed", map));
    expect(rows.map((r) => r.id)).toEqual(["a", "b", "c"]);
  });

  it("sorts by usage count descending with zero-use rows last", () => {
    const rows = [item("c", "c"), item("b", "b"), item("a", "a")];
    rows.sort((x, y) => compareByUsageSort(x, y, "usageCount", map));
    expect(rows.map((r) => r.id)).toEqual(["a", "b", "c"]);
  });

  it("breaks ties on name when usage metrics match", () => {
    const tieMap = new Map<string, UsageStats>([
      ["z", stats("z", 3, "2026-07-20T12:00:00Z")],
      ["m", stats("m", 3, "2026-07-20T12:00:00Z")],
    ]);
    const rows = [item("z", "z-skill"), item("m", "m-skill")];
    rows.sort((x, y) => compareByUsageSort(x, y, "usageCount", tieMap));
    expect(rows.map((r) => r.id)).toEqual(["m", "z"]);
  });

  it("falls back to name order when no stats exist", () => {
    const rows = [item("b", "b"), item("a", "a")];
    rows.sort((x, y) => compareByUsageSort(x, y, "lastUsed", new Map()));
    expect(rows.map((r) => r.id)).toEqual(["a", "b"]);
  });
});

function topRow(
  capabilityId: string,
  name: string,
  executionCount: number,
  lastUsedAt: string | null,
): UsageTopRow {
  return {
    capabilityId,
    name,
    kind: "skill",
    sourceLabel: "Shared",
    relativePath: name,
    capabilityScope: "global",
    workspaceRoot: null,
    executionCount,
    lastUsedAt,
    toolBuckets: [],
  };
}

describe("compareUsageTopRows", () => {
  it("sorts by latest use descending by default semantics", () => {
    const rows = [
      topRow("b", "b", 1, "2026-07-19T12:00:00Z"),
      topRow("a", "a", 3, "2026-07-20T12:00:00Z"),
    ];
    rows.sort((x, y) => compareUsageTopRows(x, y, "lastUsed"));
    expect(rows.map((r) => r.capabilityId)).toEqual(["a", "b"]);
  });

  it("sorts by usage count descending", () => {
    const rows = [
      topRow("b", "b", 1, "2026-07-20T12:00:00Z"),
      topRow("a", "a", 3, "2026-07-19T12:00:00Z"),
    ];
    rows.sort((x, y) => compareUsageTopRows(x, y, "usageCount"));
    expect(rows.map((r) => r.capabilityId)).toEqual(["a", "b"]);
  });
});
