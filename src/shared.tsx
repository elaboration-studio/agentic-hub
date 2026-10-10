// Shared constants and pure helpers used across the manager, suites, config,
// and workspace views. Presentational components live under components/.

import type {
  CapabilityItem,
  CapabilityKind,
  LinkState,
  Settings,
  SourceConfig,
  ToolId,
} from "./types";

export type Scope = "global" | "suite" | "workspace";
export type Route = "manager" | "suites" | "skills" | "statistics" | "config";
export type View = "flat" | "tree";
export type KindFilter = "all" | CapabilityKind;
export type UsageSort = "lastUsed" | "usageCount";

export const USAGE_SORT_LABEL: Record<UsageSort, string> = {
  lastUsed: "Latest use",
  usageCount: "Usage count",
};

export interface ToolDef {
  id: ToolId;
  label: string;
}

export const TOOL_LABELS: Record<ToolId, string> = {
  codex: "Codex",
  claude: "Claude",
  cursor: "Cursor",
  openclaw: "OpenClaw",
  openstandard: "OpenStandard",
  kiro: "Kiro",
  copilot: "Copilot",
  antigravity: "Antigravity",
  grok: "Grok",
};

export const ALL_TOOLS: ToolDef[] = (Object.keys(TOOL_LABELS) as ToolId[]).map(
  (id) => ({ id, label: TOOL_LABELS[id] }),
);

export const WORKSPACE_TOOL_IDS: ReadonlySet<ToolId> = new Set<ToolId>([
  "codex",
  "claude",
  "cursor",
  "kiro",
  "copilot",
  "antigravity",
  "grok",
]);

/// The fixed tool columns shown in workspace scope, in canonical order. Unlike
/// the global matrix these are not gated by `settings.tools[*].enabled`: a
/// workspace is audited for every supported tool regardless of global config.
export const WORKSPACE_TOOLS: ToolDef[] = ALL_TOOLS.filter((t) =>
  WORKSPACE_TOOL_IDS.has(t.id),
);

/// Tools the user has enabled in settings, in canonical order.
export function enabledTools(settings: Settings): ToolDef[] {
  return ALL_TOOLS.filter((t) => settings.tools[t.id].enabled);
}

export const KIND_ORDER: CapabilityKind[] = [
  "skill",
  "agent",
  "rule",
  "hook",
  "command",
];
export const KIND_LABEL: Record<CapabilityKind, string> = {
  skill: "Skills",
  agent: "Agents",
  rule: "Rules",
  hook: "Hooks",
  command: "Commands",
  mcp: "MCP",
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

// Workspace inventory item ids are namespaced with this prefix so a local
// resource never collides with a global one that shares the same relative path.
// The `::` matches `key()`'s delimiter, so a composite `key()` still parses the
// tool from its head. Used by the manager (to build rows) and the palette locate
// handler (to address a row by id).
export const WORKSPACE_ID_PREFIX = "ws::";

// Editor app name for the opener `openWith` arg. Mirrors agentic-core
// `EditorPref::app_name` so "Open original" honors the Config setting.
export function editorApp(settings: Settings): string | undefined {
  const e = settings.editor;
  switch (e.kind) {
    case "vscode":
      return "Visual Studio Code";
    case "cursor":
      return "Cursor";
    case "custom":
      return e.customApp?.trim() || undefined;
    default:
      return undefined;
  }
}

// The original file to open for a capability: the marker file inside a
// skill/hook folder, or the capability file itself for agents/rules.
export function originalFile(item: CapabilityItem): string {
  if (item.kind === "skill") return `${item.sourcePath}/SKILL.md`;
  if (item.kind === "hook") {
    return item.sourcePath.endsWith(".json") ? item.sourcePath : `${item.sourcePath}/hook.json`;
  }
  return item.sourcePath;
}

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
