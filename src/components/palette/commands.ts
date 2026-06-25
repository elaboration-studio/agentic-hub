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
  ToolCapabilityState,
  ToolId,
  WorkspaceTarget,
} from "@/types";
import { editorApp, enabledTools, key, originalFile } from "@/shared";
import { useApplyStore } from "@/state/apply";
import {
  copyText,
  emitHubWatcherChanged,
  openPath,
  readCapabilityBody,
  revealPath,
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
  /// Optional alternate action (Alt+Enter). Resource rows use it to open the
  /// source file for editing (plain Enter drills into the tool toggle panel);
  /// commands use it the other way around (Enter copies, Alt+Enter edits).
  altRun?: () => Promise<void> | void;
  /// Whether running the item dismisses the palette. Defaults to true.
  /// Navigation/drill-in rows (e.g. a search mode) set this to false to stay open.
  dismissOnRun?: boolean;
  /// Keyboard hint beside the row (e.g. "⌃1" for Ctrl+1).
  shortcut?: string;
  /// Toggle state accessory for capability-tools rows: a check (on), a hollow
  /// dot (off), or a lock (suite-managed, non-interactive). Absent on other rows.
  state?: "on" | "off" | "locked";
}

/** Whether the palette window should hide after an item's run() completes. */
export function shouldDismissPaletteAfterRun(
  item: Pick<PaletteItem, "dismissOnRun"> | undefined,
  hasPendingApplyConfirm: boolean,
): boolean {
  if (hasPendingApplyConfirm) return false;
  if (!item) return true;
  return item.dismissOnRun !== false;
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
  /// Drill into the capability-tools view (per-tool toggle panel) for a resource.
  /// `fromMode` is the search mode the user came from, so Back returns there.
  enterCapabilityTools: (item: CapabilityItem, fromMode: SearchMode) => void;
  /// Surface an item in the Hub's matrix (global or workspace scope).
  locate: (req: LocateRequest) => void;
}

/// Suite-managed cell info needed to lock a tool row (structurally compatible
/// with the manager's `OwnershipInfo`).
export interface CapabilityOwnership {
  suiteName: string;
}

/// Inputs for the capability-tools panel. The store resolves the drilled-in item
/// and supplies the inspected per-tool state plus the toggle callbacks.
export interface CapabilityToolsContext {
  settings: Settings;
  query: string;
  itemId: string;
  itemName: string;
  /// The resolved item, for the Open/Reveal action rows. Absent if not found.
  item: CapabilityItem | undefined;
  /// True once the background inspect has populated `currentMap`/`ownership`.
  inspected: boolean;
  /// `key(tool, itemId)` -> live disk state. Empty until inspected.
  currentMap: Map<string, ToolCapabilityState>;
  /// `key(tool, itemId)` -> owning suite (locked cells).
  ownership: Map<string, CapabilityOwnership>;
  /// Flip one tool for this item and apply immediately.
  toggleCapability: (tool: ToolId, itemId: string) => Promise<void> | void;
  /// Enable/disable this item across every (unlocked) enabled tool at once.
  toggleCapabilityAll: (itemId: string, enable: boolean) => Promise<void> | void;
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
    subtitle: "Enter to toggle tools · Alt+Enter to edit",
    placeholder: "Search all resources…",
  },
  skill: {
    title: "Search skills",
    subtitle: "Enter to toggle tools · Alt+Enter to edit SKILL.md",
    placeholder: "Search skills…",
  },
  agent: {
    title: "Search agents",
    subtitle: "Enter to toggle tools · Alt+Enter to edit",
    placeholder: "Search agents…",
  },
  rule: {
    title: "Search rules",
    subtitle: "Enter to toggle tools · Alt+Enter to edit",
    placeholder: "Search rules…",
  },
  hook: {
    title: "Search hooks",
    subtitle: "Enter to toggle tools · Alt+Enter to edit the manifest",
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

/// Search modes in hub order; Ctrl+1…Ctrl+7 jump directly into each mode.
export const SEARCH_MODES: SearchMode[] = [
  "all",
  "skill",
  "agent",
  "rule",
  "hook",
  "command",
  "suite",
];

export function searchModeFromShortcut(digit: number): SearchMode | null {
  const mode = SEARCH_MODES[digit - 1];
  return mode ?? null;
}

export function searchModeShortcutLabel(digit: number): string {
  return `⌃${digit}`;
}
const GOTO_MODES: SearchMode[] = ["global", "workspace"];

const NAV_TARGETS: { route: NavRoute; title: string; subtitle: string }[] = [
  { route: "manager", title: "Open Manager", subtitle: "Capability matrix" },
  { route: "suites", title: "Open Suites", subtitle: "Named capability sets" },
  { route: "config", title: "Open Config", subtitle: "Settings & shortcut" },
];

function modeRow(
  mode: SearchMode,
  section: string,
  enterMode: (m: SearchMode) => void,
  shortcutDigit?: number,
): PaletteItem {
  const def = MODE_DEFS[mode];
  return {
    id: `mode:${mode}`,
    title: def.title,
    subtitle: def.subtitle,
    group: section,
    section,
    dismissOnRun: false,
    shortcut: shortcutDigit ? searchModeShortcutLabel(shortcutDigit) : undefined,
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
    ...SEARCH_MODES.map((m, i) => modeRow(m, "Search", ctx.enterMode, i + 1)),
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

// Kind-scoped resource search (null = every non-command kind). Enter drills into
// the per-tool toggle panel; Alt+Enter opens the original file in the editor.
// Returns nothing on an empty query so the panel does not dump the whole tree.
function resourceResults(
  ctx: ProviderContext,
  kind: CapabilityItem["kind"] | null,
  fromMode: SearchMode,
): PaletteItem[] {
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
      dismissOnRun: false,
      run: () => ctx.enterCapabilityTools(it, fromMode),
      altRun: () => openPath(originalFile(it), app),
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
        return resourceResults(ctx, null, "all");
      case "skill":
      case "agent":
      case "rule":
      case "hook":
        return resourceResults(ctx, mode, mode);
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
      subtitle: `Full reset · prompts when extras exist · ${suiteName} → ${t.label}`,
      group: "Apply",
      run: () => applyToTool(suiteId, t.id, suiteName),
    }));
}

function applyToTool(suiteId: string, tool: ToolId, suiteName: string): Promise<void> {
  return useApplyStore.getState().request(tool, suiteId, suiteName);
}

/// Capability-tools view: per-tool on/off toggles for one resource, applied
/// inline. The "Tools" section lists an aggregate "all tools" row plus one row
/// per enabled tool that has a projection target (suite-managed cells render as
/// locked). The "Actions" section opens or reveals the source file. Both the
/// aggregate row and the action rows hide while a query is filtering the tools.
export function computeCapabilityToolResults(ctx: CapabilityToolsContext): PaletteItem[] {
  const { settings, query, itemId, itemName, item, inspected, currentMap, ownership } = ctx;
  const q = query.trim();
  const app = editorApp(settings);

  // Action rows never need the inspect data, so they show immediately.
  const actionRows: PaletteItem[] =
    !q && item
      ? [
          {
            id: `capopen:${itemId}`,
            title: "Open in editor",
            subtitle: originalFile(item),
            group: "Action",
            section: "Actions",
            run: () => openPath(originalFile(item), app),
          },
          {
            id: `capreveal:${itemId}`,
            title: "Reveal in Finder",
            subtitle: item.sourcePath,
            group: "Action",
            section: "Actions",
            run: () => revealPath(originalFile(item)),
          },
        ]
      : [];

  if (!inspected) {
    return [
      {
        id: `captool-loading:${itemId}`,
        title: "Loading tool states…",
        group: "Tool",
        section: "Tools",
        dismissOnRun: false,
        run: () => {},
      },
      ...actionRows,
    ];
  }

  // One row per enabled tool that actually has a projection target for this item
  // (mirrors the manager's `currentMap.has(k)` gate — e.g. a hook only targets
  // the tools its manifest opts into).
  const toolRows: PaletteItem[] = enabledTools(settings)
    .filter((t) => currentMap.has(key(t.id, itemId)))
    .filter((t) => !q || matches(`${t.label} ${t.id}`, q))
    .map((t) => {
      const k = key(t.id, itemId);
      const owner = ownership.get(k);
      const on = currentMap.get(k)?.state === "enabled";
      return {
        id: `captool:${itemId}:${t.id}`,
        title: t.label,
        subtitle: owner
          ? `Locked by suite: ${owner.suiteName}`
          : on
            ? "Enabled · Enter to disable"
            : "Disabled · Enter to enable",
        group: "Tool",
        section: "Tools",
        state: owner ? "locked" : on ? "on" : "off",
        dismissOnRun: false,
        // Locked cells are non-interactive — a suite binding owns them.
        run: owner ? () => {} : () => ctx.toggleCapability(t.id, itemId),
      } satisfies PaletteItem;
    });

  // Aggregate row: enable everywhere unless every unlocked tool is already on,
  // in which case it disables everywhere. Hidden while filtering by a query.
  const togglable = enabledTools(settings).filter(
    (t) => currentMap.has(key(t.id, itemId)) && !ownership.has(key(t.id, itemId)),
  );
  const allOn =
    togglable.length > 0 &&
    togglable.every((t) => currentMap.get(key(t.id, itemId))?.state === "enabled");
  const allRow: PaletteItem[] =
    !q && togglable.length > 0
      ? [
          {
            id: `captool-all:${itemId}`,
            title: allOn ? "Disable for all tools" : "Enable for all tools",
            subtitle: `${itemName} → ${togglable.map((t) => t.label).join(", ")}`,
            group: "Tool",
            section: "Tools",
            state: allOn ? "on" : "off",
            dismissOnRun: false,
            run: () => ctx.toggleCapabilityAll(itemId, !allOn),
          },
        ]
      : [];

  return [...allRow, ...toolRows, ...actionRows];
}
