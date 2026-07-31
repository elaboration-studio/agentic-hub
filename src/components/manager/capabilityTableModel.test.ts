import { describe, expect, it } from "vitest";

import type { CapabilityItem } from "@/types";
import {
  buildCapabilityTree,
  collectCapabilityFolderPaths,
  filterCapabilityItems,
} from "./capabilityTableModel";

function item(
  id: string,
  relativePath: string,
  kind: CapabilityItem["kind"] = "skill",
  sourceId = "shared",
): CapabilityItem {
  const segments = relativePath.split("/");
  return {
    id,
    kind,
    name: segments[segments.length - 1] ?? relativePath,
    sourcePath: `/source/${relativePath}`,
    relativePath,
    sourceId,
    sourceLabel: sourceId === "shared" ? "Shared" : "Team",
    source: { relHome: "~/.agentic", folder: ".agentic" },
    valid: true,
    validationErrors: [],
  };
}

describe("capability table model", () => {
  it("filters by query, source, kind, and caller-provided enabled ids", () => {
    const items = [
      item("skill:auth", "backend/auth"),
      item("skill:web", "frontend/web", "skill", "team"),
      item("rule:auth", "backend/auth", "rule"),
    ];

    expect(
      filterCapabilityItems(items, {
        query: "auth",
        source: "shared",
        kind: "skill",
        enabledOnly: true,
        enabledItemIds: new Set(["skill:auth"]),
      }).map((row) => row.id),
    ).toEqual(["skill:auth"]);
  });

  it("builds nested folders and reports every collapsible folder path", () => {
    const root = buildCapabilityTree([
      item("skill:review", "dev/cto/review"),
      item("skill:qa", "dev/cto/qa"),
      item("skill:write", "writing/write"),
    ]);

    expect(collectCapabilityFolderPaths(root)).toEqual(["dev", "dev/cto", "writing"]);
    expect(root.children.get("dev")?.children.get("cto")?.children.size).toBe(2);
  });
});
