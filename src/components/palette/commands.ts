// Extensible command registry for the palette. Each provider receives the
// current context (loaded resources, suites, settings, the query, and
// navigate/enterSuite callbacks) and returns ready-to-show, already
// query-filtered PaletteItems.
//
// The palette is two-level. At the root, providers search resources, suites,
// and navigation. A suite row drills into the suite-tools view (it navigates,
// it does not execute), where each row applies the suite to one tool as a full
// reset. New commands drop in as additional root providers.

import type {
  CapabilityItem,
  Settings,
  SuiteDefinition,
  ToolId,
  WorkspaceTarget,
} from "@/types";
import { editorApp, enabledTools, originalFile } from "@/shared";
import { applySuite, copyText, openPath, readCapabilityBody, type NavRoute } from "@/ipc";

/// A single actionable row in the palette.
export interface PaletteItem {
  id: string;
  title: string;
  subtitle?: string;
  /// Group label shown as a badge (e.g. "Skill", "Suite", "Apply").
  group: string;
  run: () => Promise<void> | void;
  /// Optional alternate action (Alt+Enter). Commands use it to open the source
  /// file for editing, while plain Enter copies the body to the clipboard.
  altRun?: () => Promise<void> | void;
  /// Whether running the item dismisses the palette. Defaults to true.
  /// Navigation/drill-in rows (e.g. a suite) set this to false to stay open.
  dismissOnRun?: boolean;
}

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
  /// Drill into the suite-tools view for the given suite.
  enterSuite: (suiteId: string, suiteName: string) => void;
  /// Surface a workspace inventory item in the Hub's workspace matrix.
  locate: (workspaceId: string, itemId: string) => void;
}

export type CommandProvider = (ctx: ProviderContext) => PaletteItem[];

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

// Flat resource search — the default feature. Enter opens the original file in
// the configured editor. Returns nothing on an empty query so the panel does
// not dump the whole tree. Commands are excluded here — they have their own
// provider with copy/open semantics.
const resourceSearchProvider: CommandProvider = ({ items, settings, query }) => {
  const q = query.trim();
  if (!q) return [];
  const app = editorApp(settings);
  return items
    .filter((it) => it.kind !== "command")
    .filter((it) => matches(`${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
    .map((it) => ({
      id: `resource:${it.id}`,
      title: it.name,
      subtitle: it.relativePath,
      group: GROUP_BY_KIND[it.kind],
      run: () => openPath(originalFile(it), app),
    }));
};

// Command search — slash-command prompts. Enter copies the command body to the
// clipboard (standalone use); Alt+Enter opens the source file for editing.
// Returns nothing on an empty query, like resource search.
const commandSearchProvider: CommandProvider = ({ items, settings, query }) => {
  const q = query.trim();
  if (!q) return [];
  const app = editorApp(settings);
  return items
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
};

// Workspace search — match inventory items across every remembered workspace.
// Running a row locates the item in the Hub (it does not open a file), so the
// user lands on that row in the workspace matrix. Returns nothing on an empty
// query, like resource search, so the panel does not dump every project.
const workspaceSearchProvider: CommandProvider = ({ workspaces, query, locate }) => {
  const q = query.trim();
  if (!q) return [];
  return workspaces.flatMap(({ target, items }) =>
    items
      .filter((it) => matches(`${target.label} ${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
      .map((it) => ({
        id: `workspace:${target.id}:${it.id}`,
        title: it.name,
        subtitle: `${target.label} · ${it.relativePath}`,
        group: "Workspace",
        run: () => locate(target.id, it.id),
      })),
  );
};

// Suite apply — match suites by name/description. Running a suite row drills
// into the suite-tools view (pick a tool to apply to) rather than executing.
// Shown on an empty query so the user can browse suites.
const suiteApplyProvider: CommandProvider = ({ suites, query, enterSuite }) => {
  const q = query.trim();
  return suites
    .filter((s) => !q || matches(`${s.name} ${s.description ?? ""}`, q))
    .map((s) => ({
      id: `suite:${s.id}`,
      title: s.name,
      subtitle: `${s.capabilities.length} capabilities${s.description ? ` · ${s.description}` : ""}`,
      group: "Suite",
      dismissOnRun: false,
      run: () => enterSuite(s.id, s.name),
    }));
};

const NAV_TARGETS: { route: NavRoute; title: string; subtitle: string }[] = [
  { route: "manager", title: "Open Manager", subtitle: "Capability matrix" },
  { route: "suites", title: "Open Suites", subtitle: "Named capability sets" },
  { route: "config", title: "Open Config", subtitle: "Settings & shortcut" },
];

// Navigation commands — always available; shown on an empty query and filtered
// by it otherwise.
const navProvider: CommandProvider = ({ query, navigate }) => {
  const q = query.trim();
  return NAV_TARGETS.filter((n) => !q || matches(`${n.title} ${n.subtitle}`, q)).map((n) => ({
    id: `nav:${n.route}`,
    title: n.title,
    subtitle: n.subtitle,
    group: "Navigate",
    run: () => navigate(n.route),
  }));
};

/// Ordered root provider registry. Resources first so the top hit on a query
/// is a resource (the primary "search and open" flow); suites then navigation.
export const PROVIDERS: CommandProvider[] = [
  resourceSearchProvider,
  commandSearchProvider,
  workspaceSearchProvider,
  suiteApplyProvider,
  navProvider,
];

/// Cap on rendered rows — keeps the list responsive on large resource trees.
export const MAX_RESULTS = 50;

/// Compose every root provider into the final, capped result list.
export function computeResults(ctx: ProviderContext): PaletteItem[] {
  return PROVIDERS.flatMap((provider) => provider(ctx)).slice(0, MAX_RESULTS);
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
