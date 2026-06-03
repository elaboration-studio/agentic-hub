// Extensible command registry for the palette. Each provider receives the
// current context (loaded resources, settings, the query, and a navigate
// callback) and returns ready-to-show, already query-filtered PaletteItems.
// New commands — install a skill from a vendor, switch a suite to a tool —
// drop in as additional providers with no change to the UI or the store.

import type { CapabilityItem, Settings } from "@/types";
import { editorApp, originalFile } from "@/shared";
import { openPath, type NavRoute } from "@/ipc";

/// A single actionable row in the palette.
export interface PaletteItem {
  id: string;
  title: string;
  subtitle?: string;
  /// Group label shown as a badge (e.g. "Skill", "Navigate").
  group: string;
  run: () => Promise<void> | void;
}

export interface ProviderContext {
  settings: Settings;
  items: CapabilityItem[];
  query: string;
  navigate: (route: NavRoute) => void;
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
};

// Flat resource search — the default feature. Enter opens the original file in
// the configured editor. Returns nothing on an empty query so the panel does
// not dump the whole tree.
const resourceSearchProvider: CommandProvider = ({ items, settings, query }) => {
  const q = query.trim();
  if (!q) return [];
  const app = editorApp(settings);
  return items
    .filter((it) => matches(`${it.name} ${it.relativePath} ${it.sourceLabel}`, q))
    .map((it) => ({
      id: `resource:${it.id}`,
      title: it.name,
      subtitle: it.relativePath,
      group: GROUP_BY_KIND[it.kind],
      run: () => openPath(originalFile(it), app),
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

/// Ordered provider registry. Resources first so the top hit on a query is a
/// resource (the primary "search and open" flow); navigation follows.
export const PROVIDERS: CommandProvider[] = [resourceSearchProvider, navProvider];

/// Cap on rendered rows — keeps the list responsive on large resource trees.
export const MAX_RESULTS = 50;

/// Compose every provider into the final, capped result list.
export function computeResults(ctx: ProviderContext): PaletteItem[] {
  return PROVIDERS.flatMap((provider) => provider(ctx)).slice(0, MAX_RESULTS);
}
