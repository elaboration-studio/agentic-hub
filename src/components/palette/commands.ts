// Layered command registry for the palette. The root view is a hub of
// first-class commands grouped into sections (Search / Go to / Navigate /
// Actions); typing at the root filters the hub rows only — never resources.
// Resource results appear after drilling into a search mode, so a query
// targets exactly one slice; cross-kind search is the explicit "all" mode.
//
// A suite row (inside the suite search mode) drills further into the
// suite-tools view, where each row applies the suite to one tool as a full
// reset. New commands drop in as additional hub rows.

import type {
  CapabilityItem,
  Settings,
  SuiteDefinition,
  ToolId,
  WorkspaceTarget,
} from "@/types";
import { editorApp, enabledTools, originalFile } from "@/shared";
import {
  applySuite,
  copyText,
  emitHubWatcherChanged,
  openPath,
  readCapabilityBody,
  setWatcherEnabled,
  type LocateRequest,
  type NavRoute,
} from "@/ipc";

/// A single actionable row in the palette.
export interface PaletteItem {
  id: string;
  title: string;
  subtitle?: string;
  /// Group label shown as a badge (e.g. "Skill", "Suite", "Apply").
  group: string;
  /// Hub section the row renders under at the root (e.g. "Search", "Go to").
  section?: string;
  run: () => Promise<void> | void;
  /// Optional alternate action (Alt+Enter). Commands use it to open the source
  /// file for editing, while plain Enter copies the body to the clipboard.
  altRun?: () => Promise<void> | void;
  /// Whether running the item dismisses the palette. Defaults to true.
  /// Navigation/drill-in rows (e.g. a search mode) set this to false to stay open.
  dismissOnRun?: boolean;
}

/// One drillable palette mode: a kind-scoped search, the explicit cross-kind
/// "all" search, the suite-apply flow, or a locate scope (global/workspace).
export type SearchMode =
  | "all"
  | "skill"
  | "agent"
  | "rule"
  | "hook"
  | "command"
  | "suite"
  | "global"
  | "workspace";

/// One remembered workspace and the read-only inventory scanned from its tool
/// dirs. Loaded per summon so the palette can search across every project.
export interface WorkspaceInventoryEntry {
  target: WorkspaceTarget;
  items: CapabilityItem[];
}

export interface ProviderContext {
  settings: Settings;
  items: CapabilityItem[];
  suites: SuiteDefinition[];
  workspaces: WorkspaceInventoryEntry[];
  query: string;
  navigate: (route: NavRoute) => void;
  /// Drill into a search / go-to mode.
  enterMode: (mode: SearchMode) => void;
  /// Drill into the suite-tools view for the given suite.
  enterSuite: (suiteId: string, suiteName: string) => void;
  /// Surface an item in the Hub's matrix (global or workspace scope).
  locate: (req: LocateRequest) => void;
}

/// Case-insensitive substring match over a precomposed haystack.
function matches(haystack: string, q: string): boolean {
  return haystack.toLowerCase().includes(q.toLowerCase());
}

const GROUP_BY_KIND: Record<CapabilityItem["kind"], string> = {
  skill: "Skill",
  agent: "Agent",
  rule: "Rule",
  hook: "Hook",
  command: "Command",
};

interface ModeDef {
  /// Hub row title; also the breadcrumb label inside the mode.
  title: string;
  subtitle: string;
  /// Input placeholder shown inside the mode.
  placeholder: string;
}

export const MODE_DEFS: Record<SearchMode, ModeDef> = {
  all: {
    title: "Search all resources",
    subtitle: "Skills, agents, rules & hooks across every source",
    placeholder: "Search all resources…",
  },
  skill: {
    title: "Search skills",
    subtitle: "Open a skill's SKILL.md in your editor",
    placeholder: "Search skills…",
  },
  agent: {
    title: "Search agents",
    subtitle: "Open an agent file in your editor",
    placeholder: "Search agents…",
  },
  rule: {
    title: "Search rules",
    subtitle: "Open a rule file in your editor",
    placeholder: "Search rules…",
  },
  hook: {
    title: "Search hooks",
    subtitle: "Open a hook manifest in your editor",
    placeholder: "Search hooks…",
  },
  command: {
    title: "Search commands",
    subtitle: "Enter copies the body · Alt+Enter edits",
    placeholder: "Search commands…",
  },
  suite: {
    title: "Search suites",
    subtitle: "Pick a suite, then a tool to apply it to",
    placeholder: "Search suites…",
  },
  global: {
    title: "Go to global",
    subtitle: "Locate a shared resource in the Manager matrix",
    placeholder: "Find in global…",
  },
  workspace: {
    title: "Go to workspace",
    subtitle: "Locate an item across every remembered project",
    placeholder: "Find in workspace…",
  },
};

const SEARCH_MODES: SearchMode[] = ["all", "skill", "agent", "rule", "hook", "command", "suite"];
const GOTO_MODES: SearchMode[] = ["global", "workspace"];

const NAV_TARGETS: { route: NavRoute; title: string; subtitle: string }[] = [
  { route: "manager", title: "Open Manager", subtitle: "Capability matrix" },
  { route: "suites", title: "Open Suites", subtitle: "Named capability sets" },
  { route: "config", title: "Open Config", subtitle: "Settings & shortcut" },
];

function modeRow(mode: SearchMode, section: string, enterMode: (m: SearchMode) => void): PaletteItem {
  const def = MODE_DEFS[mode];
  return {
    id: `mode:${mode}`,
    title: def.title,
    subtitle: def.subtitle,
    group: section,
    section,
    dismissOnRun: false,
    run: () => enterMode(mode),
  };
}

// Pause/Resume watching — a terminal hub action. Persists the flipped state
// through the same IPC as the header toggle, then notifies the main window so
// its header stays in sync without surfacing it.
function watchingRow(settings: Settings): PaletteItem {
  const next = !settings.watcherEnabled;
  return {
    id: "action:toggle-watching",
    title: settings.watcherEnabled ? "Pause watching" : "Resume watching",
    subtitle: settings.watcherEnabled
      ? "Stop auto-syncing projections on source changes"
      : "Resume auto-syncing projections on source changes",
    group: "Action",
    section: "Actions",
    run: async () => {
      await setWatcherEnabled(next);
      await emitHubWatcherChanged(next);
    },
  };
}

/// Root hub: the categorized first-class commands. Typing filters these rows
/// only (section + title + subtitle) — no resource results at the root.
export function computeHubResults(ctx: ProviderContext): PaletteItem[] {
  const q = ctx.query.trim();
  const rows: PaletteItem[] = [
    ...SEARCH_MODES.map((m) => modeRow(m, "Search", ctx.enterMode)),
    ...GOTO_MODES.map((m) => modeRow(m, "Go to", ctx.enterMode)),
    ...NAV_TARGETS.map((n) => ({
      id: `nav:${n.route}`,
      title: n.title,
      subtitle: n.subtitle,
      group: "Navigate",
      section: "Navigate",
      run: () => ctx.navigate(n.route),
    })),
    {
      id: "action:apply-suite",
      title: "Apply suite…",
      subtitle: "Pick a suite, then a tool — full reset",
      group: "Action",
      section: "Actions",
      dismissOnRun: false,
      run: () => ctx.enterMode("suite"),
    },
    watchingRow(ctx.settings),
  ];
  return rows.filter((r) => !q || matches(`${r.section} ${r.title} ${r.subtitle ?? ""}`, q));
}

// Kind-scoped resource search (null = every non-command kind). Enter opens the
// original file in the configured editor. Returns nothing on an empty query so
// the panel does not dump the whole tree.
function resourceResults(ctx: ProviderContext, kind: CapabilityItem["kind"] | null): PaletteItem[] {
  const q = ctx.query.trim();
  if (!q) return [];
  const app = editorApp(ctx.settings);
  return ctx.items
    .filter((it) => (kind ? it.kind === kind : it.kind !== "command"))
    .filter((it) => matches(`${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
    .map((it) => ({
      id: `resource:${it.id}`,
      title: it.name,
      subtitle: it.relativePath,
      group: GROUP_BY_KIND[it.kind],
      run: () => openPath(originalFile(it), app),
    }));
}

// Command search — slash-command prompts. Enter copies the command body to the
// clipboard (standalone use); Alt+Enter opens the source file for editing.
function commandResults(ctx: ProviderContext): PaletteItem[] {
  const q = ctx.query.trim();
  if (!q) return [];
  const app = editorApp(ctx.settings);
  return ctx.items
    .filter((it) => it.kind === "command")
    .filter((it) => matches(`${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
    .map((it) => {
      const file = originalFile(it);
      return {
        id: `command:${it.id}`,
        title: it.name,
        subtitle: `${it.relativePath} · Enter to copy, Alt+Enter to edit`,
        group: GROUP_BY_KIND.command,
        run: async () => {
          const body = await readCapabilityBody(file);
          await copyText(body);
        },
        altRun: () => openPath(file, app),
      };
    });
}

// Suite search — running a suite row drills into the suite-tools view (pick a
// tool to apply to) rather than executing. Lists every suite on an empty query
// (the set is small and the apply action lands the user here to browse).
function suiteResults(ctx: ProviderContext): PaletteItem[] {
  const q = ctx.query.trim();
  return ctx.suites
    .filter((s) => !q || matches(`${s.name} ${s.description ?? ""}`, q))
    .map((s) => ({
      id: `suite:${s.id}`,
      title: s.name,
      subtitle: `${s.capabilities.length} capabilities${s.description ? ` · ${s.description}` : ""}`,
      group: "Suite",
      dismissOnRun: false,
      run: () => ctx.enterSuite(s.id, s.name),
    }));
}

// Go to global — locate a shared resource in the Manager matrix (global
// scope). Never opens a file. Returns nothing on an empty query.
function globalLocateResults(ctx: ProviderContext): PaletteItem[] {
  const q = ctx.query.trim();
  if (!q) return [];
  return ctx.items
    .filter((it) => matches(`${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
    .map((it) => ({
      id: `global:${it.id}`,
      title: it.name,
      subtitle: `${it.sourceLabel} · ${it.relativePath}`,
      group: "Global",
      run: () => ctx.locate({ scope: "global", itemId: it.id }),
    }));
}

// Go to workspace — locate an inventory item across every remembered
// workspace. Never opens a file. Returns nothing on an empty query.
function workspaceLocateResults(ctx: ProviderContext): PaletteItem[] {
  const q = ctx.query.trim();
  if (!q) return [];
  return ctx.workspaces.flatMap(({ target, items }) =>
    items
      .filter((it) => matches(`${target.label} ${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
      .map((it) => ({
        id: `workspace:${target.id}:${it.id}`,
        title: it.name,
        subtitle: `${target.label} · ${it.relativePath}`,
        group: "Workspace",
        run: () => ctx.locate({ scope: "workspace", workspaceId: target.id, itemId: it.id }),
      })),
  );
}

/// Cap on rendered rows — keeps the list responsive on large resource trees.
export const MAX_RESULTS = 50;

/// Results for one drilled-in search / go-to mode, capped.
export function computeSearchResults(ctx: ProviderContext, mode: SearchMode): PaletteItem[] {
  const rows = (() => {
    switch (mode) {
      case "all":
        return resourceResults(ctx, null);
      case "skill":
      case "agent":
      case "rule":
      case "hook":
        return resourceResults(ctx, mode);
      case "command":
        return commandResults(ctx);
      case "suite":
        return suiteResults(ctx);
      case "global":
        return globalLocateResults(ctx);
      case "workspace":
        return workspaceLocateResults(ctx);
    }
  })();
  return rows.slice(0, MAX_RESULTS);
}

/// Suite-tools view: one row per enabled tool that applies the suite to that
/// tool as a full reset (clean + replace). Filtered by the query.
export function computeSuiteToolResults(
  settings: Settings,
  query: string,
  suiteId: string,
  suiteName: string,
): PaletteItem[] {
  const q = query.trim();
  return enabledTools(settings)
    .filter((t) => !q || matches(`${t.label} ${t.id}`, q))
    .map((t) => ({
      id: `apply:${suiteId}:${t.id}`,
      title: `Apply to ${t.label}`,
      subtitle: `Full reset · ${suiteName} → ${t.label}`,
      group: "Apply",
      run: () => applyToTool(suiteId, t.id),
    }));
}

function applyToTool(suiteId: string, tool: ToolId): Promise<void> {
  return applySuite(tool, suiteId).then(() => undefined);
}
