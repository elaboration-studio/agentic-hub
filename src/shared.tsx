// Shared UI primitives, constants, and helpers used across the manager and
// config views. Keeps App.tsx and the panel modules thin.

import type { ReactNode } from "react";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  LinkState,
  Settings,
  SourceConfig,
  ToolCapabilityState,
  ToolId,
} from "./types";
import type { DesiredMap } from "./ipc";

export type Scope = "global" | "workspace";
export type Route = "manager" | "suites" | "config";
export type View = "flat" | "tree";

export interface ToolDef {
  id: ToolId;
  label: string;
}

export const TOOL_LABELS: Record<ToolId, string> = {
  codex: "Codex",
  claude: "Claude",
  cursor: "Cursor",
  openclaw: "OpenClaw",
};

export const ALL_TOOLS: ToolDef[] = (Object.keys(TOOL_LABELS) as ToolId[]).map(
  (id) => ({ id, label: TOOL_LABELS[id] }),
);

export const WORKSPACE_TOOL_IDS: ReadonlySet<ToolId> = new Set<ToolId>([
  "codex",
  "claude",
  "cursor",
]);

/// Tools the user has enabled in settings, in canonical order.
export function enabledTools(settings: Settings): ToolDef[] {
  return ALL_TOOLS.filter((t) => settings.tools[t.id].enabled);
}

export const KIND_ORDER: CapabilityKind[] = ["skill", "agent", "rule", "hook"];
export const KIND_LABEL: Record<CapabilityKind, string> = {
  skill: "Skills",
  agent: "Agents",
  rule: "Rules",
  hook: "Hooks",
};

// Current states that are "abnormal" — surfaced as a dot on the toggle.
export const ABNORMAL: Record<LinkState, string | null> = {
  enabled: null,
  disabled: null,
  broken: "Broken link",
  stale: "Stale copy",
  foreign_file: "A real file blocks this target",
  foreign_link: "Owned by another source",
};

export const key = (tool: ToolId, itemId: string) => `${tool}::${itemId}`;

export function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

// Mirrors agentic-core `settings::slugify` so the UI can address a source by
// the same id the backend assigns (needed to remove a source).
export function slugify(label: string): string {
  let out = "";
  let prevDash = false;
  for (const ch of label) {
    if (/[a-zA-Z0-9]/.test(ch)) {
      out += ch.toLowerCase();
      prevDash = false;
    } else if (out.length > 0 && !prevDash) {
      out += "-";
      prevDash = true;
    }
  }
  const trimmed = out.replace(/^-+|-+$/g, "");
  return trimmed === "" ? "source" : trimmed;
}

// Mirrors `Settings::resolve_sources` slug dedupe.
export function resolveSourceIds(sources: SourceConfig[]): string[] {
  const counts: Record<string, number> = {};
  return sources.map((s) => {
    const base = slugify(s.label);
    const n = (counts[base] ?? 0) + 1;
    counts[base] = n;
    return n === 1 ? base : `${base}-${n}`;
  });
}

export function Banner(props: { tone: "danger" | "muted"; children: ReactNode }) {
  return <div className={`banner banner-${props.tone}`}>{props.children}</div>;
}

// Per-tool toggle cells for one capability row. Shared by the flat and tree
// renderers so toggle behaviour stays identical across views.
export function ToolCells(props: {
  item: CapabilityItem;
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
}) {
  const { item, tools, adapterMap, currentMap, desired, onToggle } = props;
  return (
    <>
      {tools.map((t) => {
        const adapter = adapterMap.get(t.id);
        const k = key(t.id, item.id);
        const cur = currentMap.get(k);
        if ((adapter && !adapter.available) || !cur) {
          return (
            <td key={t.id} className="cell">
              <span className="dash">—</span>
            </td>
          );
        }
        const on = desired[k] ?? false;
        const modified = on !== (cur.state === "enabled");
        const abnormal = ABNORMAL[cur.state];
        return (
          <td key={t.id} className="cell">
            <button
              className={`toggle${on ? " on" : ""}${modified ? " mod" : ""}${
                abnormal ? " warn" : ""
              }`}
              title={`current: ${cur.state}${abnormal ? ` — ${abnormal}` : ""}`}
              onClick={() => onToggle(t.id, item.id)}
            >
              {on ? "✓" : ""}
              {abnormal && <span className="warn-dot" />}
            </button>
          </td>
        );
      })}
    </>
  );
}
