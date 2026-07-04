// Capability matrix with two interchangeable layouts:
//   - flat: capabilities grouped by kind (Skills / Agents / Rules / Hooks)
//   - tree: capabilities nested by their source-relative folder path
// A shared search box filters both views. Group rows (kinds in flat, folders in
// tree) carry batch toggles that flip every capability beneath them per tool.

import { Fragment, useEffect, useMemo, type ReactNode } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Database,
  FolderTree,
  List,
  Minus,
  MoreHorizontal,
  RefreshCw,
  Search,
  Tag,
} from "lucide-react";
import { toast } from "sonner";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  Settings,
  ToolCapabilityState,
  ToolId,
} from "@/types";
import { openPath, openUpdateWindow, revealPath } from "@/ipc";
import {
  editorApp,
  key,
  KIND_LABEL,
  KIND_ORDER,
  messageOf,
  originalFile,
  type KindFilter,
  type ToolDef,
  type View,
} from "@/shared";
import { useManagerStore, type OwnershipInfo } from "@/state/manager";
import { useManagerFiltersStore } from "@/state/managerFilters";
import { useWorkspaceStore } from "@/state/workspace";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { ToolCells } from "./ToolCells";

const EMPTY_COLLAPSE: ReadonlySet<string> = new Set();

// Table header pinned just below the 56px (h-14) sticky toolbar so it stays
// visible while the body scrolls. Opaque bg-card hides rows passing beneath;
// z-20 sits under the toolbar (z-30) so they never overlap.
const STICKY_HEAD = "sticky top-14 z-20 bg-card";

const KIND_BADGE_COLOR: Record<CapabilityKind, string> = {
  skill: "text-primary",
  agent: "text-kind-agent",
  rule: "text-success",
  hook: "text-warning",
  command: "text-kind-command",
};

export function Matrix() {
  const data = useManagerStore((s) => s.data);
  const tools = useManagerStore((s) => s.tools);
  const currentMap = useManagerStore((s) => s.currentMap);
  const desired = useManagerStore((s) => s.desired);
  const ownership = useManagerStore((s) => s.ownership);
  const onToggle = useManagerStore((s) => s.toggle);
  const onToggleMany = useManagerStore((s) => s.toggleMany);
  const readOnly = useManagerStore((s) => s.readOnly);
  const readOnlyItemIds = useManagerStore((s) => s.readOnlyItemIds);
  const lockedSkills = useManagerStore((s) => s.lockedSkills);
  const workspaceId = useWorkspaceStore((s) => s.activeId);

  const view = useManagerFiltersStore((s) => s.view);
  const setView = useManagerFiltersStore((s) => s.setView);
  const collapsed = useManagerFiltersStore((s) => s.collapsed);
  const setCollapsed = useManagerFiltersStore((s) => s.setCollapsed);
  const toggleCollapsed = useManagerFiltersStore((s) => s.toggleCollapsed);
  const query = useManagerFiltersStore((s) => s.query);
  const setQuery = useManagerFiltersStore((s) => s.setQuery);
  const source = useManagerFiltersStore((s) => s.source);
  const setSource = useManagerFiltersStore((s) => s.setSource);
  const kind = useManagerFiltersStore((s) => s.kind);
  const setKind = useManagerFiltersStore((s) => s.setKind);
  const enabledOnly = useManagerFiltersStore((s) => s.enabledOnly);
  const setEnabledOnly = useManagerFiltersStore((s) => s.setEnabledOnly);
  const locateId = useManagerFiltersStore((s) => s.locateId);
  const clearLocate = useManagerFiltersStore((s) => s.clearLocate);

  const items = data?.items ?? [];
  const adapterStatuses = data?.result.adapterStatuses ?? [];

  const adapterMap = useMemo(() => {
    const m = new Map<ToolId, AdapterStatus>();
    for (const a of adapterStatuses) m.set(a.tool, a);
    return m;
  }, [adapterStatuses]);

  const sources = useMemo(() => {
    const seen = new Map<string, string>();
    for (const it of items) if (!seen.has(it.sourceId)) seen.set(it.sourceId, it.sourceLabel);
    return [...seen].map(([id, label]) => ({ id, label }));
  }, [items]);

  // Item ids enabled (checked) in at least one tool — drives the "enabled only"
  // filter. Keyed on `desired` so unapplied toggles count too.
  const enabledItemIds = useMemo(() => {
    const ids = new Set<string>();
    for (const it of items) {
      for (const t of tools) {
        const k = key(t.id, it.id);
        if (currentMap.has(k) && desired[k]) {
          ids.add(it.id);
          break;
        }
      }
    }
    return ids;
  }, [items, tools, currentMap, desired]);

  const filtered = useMemo(
    () => filterItems(items, query, source, kind, enabledOnly, enabledItemIds),
    [items, query, source, kind, enabledOnly, enabledItemIds],
  );
  const root = useMemo(() => buildTree(filtered), [filtered]);
  const folderPaths = useMemo(() => collectFolderPaths(root), [root]);

  // Locate flow (from a palette workspace search): expand the row's ancestor
  // folders so a tree-collapsed match becomes visible, scroll it into view, and
  // clear the highlight after a moment. Reads collapsed via getState so the
  // effect only re-runs when the located item changes, not on every expand.
  useEffect(() => {
    if (!locateId || !data) return;
    const target = data.items.find((it) => it.id === locateId);
    if (!target) return;

    const ancestors = ancestorPaths(target.relativePath);
    const { collapsed: cur, setCollapsed: set } = useManagerFiltersStore.getState();
    if (ancestors.some((p) => cur.has(p))) {
      set(new Set([...cur].filter((p) => !ancestors.includes(p))));
    }

    const scroll = setTimeout(() => {
      document
        .querySelector("[data-locate-row]")
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    }, 80);
    const clear = setTimeout(() => clearLocate(), 2200);
    return () => {
      clearTimeout(scroll);
      clearTimeout(clear);
    };
  }, [locateId, data, clearLocate]);

  if (!data || items.length === 0) {
    return (
      <Alert>
        <AlertDescription>
          {readOnly
            ? "No agentic resources apply to this project — nothing installed locally or projected from a shared source yet."
            : "No capabilities found in the configured sources."}
        </AlertDescription>
      </Alert>
    );
  }

  const ctx: BodyContext = {
    tools,
    adapterMap,
    currentMap,
    desired,
    ownership,
    onToggle,
    onToggleMany,
    settings: data.settings,
    readOnly,
    readOnlyItemIds,
    locateId,
    lockedSkills,
    workspaceId,
  };
  const effectiveCollapsed = query.trim() ? EMPTY_COLLAPSE : collapsed;

  return (
    <section className="flex min-w-fit flex-col gap-2.5">
      <div className="sticky top-0 z-30 flex h-14 items-center gap-2.5 bg-background shadow-[0_-1.25rem_0_0_var(--background)]">
        <ToggleGroup
          type="single"
          value={view}
          onValueChange={(v) => v && setView(v as View)}
          variant="outline"
        >
          <ToggleGroupItem value="flat" aria-label="Flat view — grouped by kind" title="Flat — by kind">
            <List />
          </ToggleGroupItem>
          <ToggleGroupItem value="tree" aria-label="Tree view — grouped by folder" title="Tree — by folder">
            <FolderTree />
          </ToggleGroupItem>
        </ToggleGroup>
        <div className="relative flex-1">
          <Search
            className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            type="search"
            className="pl-9"
            placeholder="Search by name, path, or source…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <Select value={kind} onValueChange={(v) => setKind(v as KindFilter)}>
          <SelectTrigger className="w-[140px]" title="Filter by type">
            <span className="flex min-w-0 items-center gap-2">
              <Tag />
              <SelectValue />
            </span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All types</SelectItem>
            <SelectItem value="skill">Skills</SelectItem>
            <SelectItem value="agent">Agents</SelectItem>
            <SelectItem value="rule">Rules</SelectItem>
            <SelectItem value="hook">Hooks</SelectItem>
            <SelectItem value="command">Commands</SelectItem>
          </SelectContent>
        </Select>
        {sources.length > 1 && (
          <Select value={source || "all"} onValueChange={(v) => setSource(v === "all" ? "" : v)}>
            <SelectTrigger className="w-[160px]" title="Filter by source">
              <span className="flex min-w-0 items-center gap-2">
                <Database />
                <SelectValue />
              </span>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All sources</SelectItem>
              {sources.map((s) => (
                <SelectItem key={s.id} value={s.id}>
                  {s.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        {/* In read-only inventory every shown resource is present (enabled),
            so an "enabled only" toggle would be a no-op — hide it. */}
        {!readOnly && (
          <Label className="flex shrink-0 items-center gap-2 text-muted-foreground">
            <Checkbox
              checked={enabledOnly}
              onCheckedChange={(v) => setEnabledOnly(v === true)}
            />
            Enabled only
          </Label>
        )}
        {view === "tree" && (
          <div className="flex gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Expand all folders"
                  onClick={() => setCollapsed(new Set())}
                >
                  <ChevronsUpDown />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Expand all</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Collapse all folders"
                  onClick={() => setCollapsed(new Set(folderPaths))}
                >
                  <ChevronsDownUp />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Collapse all</TooltipContent>
            </Tooltip>
          </div>
        )}
      </div>
      {filtered.length === 0 ? (
        <Alert>
          <AlertDescription>No capabilities match “{query.trim()}”.</AlertDescription>
        </Alert>
      ) : (
        <Table containerClassName="overflow-x-visible overflow-y-visible rounded-xl border bg-card">
          <TableHeader>
            <TableRow className="hover:bg-card">
              <TableHead className={STICKY_HEAD}>Capability</TableHead>
              <TableHead className={cn("w-32", STICKY_HEAD)}>Source</TableHead>
              {tools.map((t) => {
                const adapter = adapterMap.get(t.id);
                const off = adapter && !adapter.available;
                return (
                  <TableHead
                    key={t.id}
                    className={cn("w-[120px] text-center", STICKY_HEAD)}
                    title={adapter?.unavailableReason ?? ""}
                  >
                    {t.label}
                    {off && (
                      <span className="ml-1.5 rounded border px-1 text-[9px] uppercase text-muted-foreground">
                        off
                      </span>
                    )}
                  </TableHead>
                );
              })}
            </TableRow>
          </TableHeader>
          <TableBody>
            {view === "flat"
              ? renderFlat(filtered, ctx)
              : renderNodes(
                  [...root.children.values()],
                  0,
                  ctx,
                  effectiveCollapsed,
                  toggleCollapsed,
                )}
          </TableBody>
        </Table>
      )}
    </section>
  );
}

interface BodyContext {
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: Record<string, boolean>;
  ownership: Map<string, OwnershipInfo>;
  onToggle: (tool: ToolId, itemId: string) => void;
  onToggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  settings: Settings;
  // Workspace scope: cells and aggregates render as static present/absent.
  readOnly: boolean;
  // Global installed resources are row-level read-only.
  readOnlyItemIds: ReadonlySet<string>;
  // The row to surface from a palette locate (namespaced item id), or "".
  locateId: string;
  // Namespaced item id -> skills.sh install behind it (workspace scope only).
  lockedSkills: Map<string, { name: string; source: string }>;
  // The active workspace id, needed to target an update run. "" in global scope.
  workspaceId: string;
}

// The folder paths leading to a leaf, given its source-relative path. Used to
// expand a collapsed tree down to a located row. `dev/cto/qa` -> [`dev`, `dev/cto`].
function ancestorPaths(relativePath: string): string[] {
  const parts = relativePath.split("/").filter(Boolean);
  const out: string[] = [];
  let acc = "";
  for (let i = 0; i < parts.length - 1; i++) {
    acc = acc ? `${acc}/${parts[i]}` : parts[i];
    out.push(acc);
  }
  return out;
}

function filterItems(
  items: CapabilityItem[],
  query: string,
  source: string,
  kind: KindFilter,
  enabledOnly: boolean,
  enabledItemIds: ReadonlySet<string>,
): CapabilityItem[] {
  const q = query.trim().toLowerCase();
  if (!q && !source && kind === "all" && !enabledOnly) return items;
  return items.filter((it) => {
    if (enabledOnly && !enabledItemIds.has(it.id)) return false;
    if (kind !== "all" && it.kind !== kind) return false;
    if (source && it.sourceId !== source) return false;
    if (!q) return true;
    return (
      it.name.toLowerCase().includes(q) ||
      it.relativePath.toLowerCase().includes(q) ||
      it.sourceLabel.toLowerCase().includes(q)
    );
  });
}

// Batch toggle cells for a group row (a kind or a folder subtree): one click
// flips every togglable descendant for that tool. Shows ✓ (all on), – (mixed),
// or empty (none).
function AggregateCells(props: { items: CapabilityItem[]; ctx: BodyContext }) {
  const { items, ctx } = props;
  return (
    <>
      {ctx.tools.map((t) => {
        const present = items.filter((it) => ctx.currentMap.has(key(t.id, it.id)));
        const togglable = present.filter((it) => !ctx.readOnlyItemIds.has(it.id));
        if (present.length === 0) {
          return (
            <TableCell key={t.id} className="text-center">
              <span className="text-muted-foreground/50">—</span>
            </TableCell>
          );
        }
        // Read-only inventory: a static "present / total" count, never a batch
        // toggle. Success-colored when this tool has every resource in the group.
        if (ctx.readOnly) {
          const all = togglable.length === items.length;
          return (
            <TableCell key={t.id} className="text-center">
              <span className={cn("text-xs tabular-nums", all ? "text-success" : "text-muted-foreground")}>
                {togglable.length}
              </span>
            </TableCell>
          );
        }
        if (togglable.length === 0) {
          return (
            <TableCell key={t.id} className="text-center">
              <span className="text-xs tabular-nums text-success">{present.length}</span>
            </TableCell>
          );
        }
        const onCount = togglable.filter((it) => ctx.desired[key(t.id, it.id)]).length;
        const allOn = onCount === togglable.length;
        const mixed = onCount > 0 && !allOn;
        return (
          <TableCell key={t.id} className="text-center">
            <Button
              variant="outline"
              size="icon-xs"
              className={cn(
                "size-[26px] rounded-md text-primary",
                allOn && "border-success/40 bg-success/15 text-success",
                mixed && "border-primary bg-primary/15",
              )}
              title={`${onCount}/${togglable.length} on — click to ${allOn ? "disable" : "enable"} all`}
              onClick={() =>
                ctx.onToggleMany(
                  t.id,
                  togglable.map((it) => it.id),
                  !allOn,
                )
              }
            >
              {allOn ? <Check className="size-3" /> : mixed ? <Minus className="size-3" /> : null}
            </Button>
          </TableCell>
        );
      })}
    </>
  );
}

// Hidden-until-hover row menu: open the original in the preferred editor,
// reveal it in Finder, and open the file each enabled tool actually references.
function RowActions(props: { item: CapabilityItem; ctx: BodyContext }) {
  const { item, ctx } = props;
  const app = editorApp(ctx.settings);
  const run = (p: Promise<void>) => void p.catch((e) => toast.error(messageOf(e)));

  const projected = ctx.tools
    .map((t) => ({ tool: t, state: ctx.currentMap.get(key(t.id, item.id)) }))
    .filter(
      (x): x is { tool: ToolDef; state: ToolCapabilityState } =>
        !!x.state && x.state.state === "enabled" && !!x.state.targetPath,
    );

  // skills.sh manages this row (workspace scope) — offer a one-click update.
  const locked = ctx.readOnly ? ctx.lockedSkills.get(item.id) : undefined;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          // Hidden until the row is hovered or the trigger is focused.
          className="ml-2 align-middle opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
          title="More actions"
          aria-label="More actions"
        >
          <MoreHorizontal className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {locked && ctx.workspaceId && (
          <>
            <DropdownMenuItem
              onClick={() =>
                run(
                  openUpdateWindow(
                    ctx.workspaceId,
                    "skills.sh",
                    locked.source,
                    locked.name,
                  ),
                )
              }
            >
              <RefreshCw className="size-3.5" />
              Update via skills.sh
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem onClick={() => run(openPath(originalFile(item), app))}>
          Open original
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => run(revealPath(item.sourcePath))}>
          Reveal in Finder
        </DropdownMenuItem>
        {projected.length > 0 && <DropdownMenuSeparator />}
        {projected.map(({ tool, state }) => (
          <DropdownMenuItem key={tool.id} onClick={() => run(openPath(state.targetPath))}>
            Open in {tool.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function leafRow(item: CapabilityItem, ctx: BodyContext, padding?: number, badge?: boolean) {
  const located = !!ctx.locateId && item.id === ctx.locateId;
  return (
    <TableRow
      key={`l:${item.id}`}
      className={cn(
        "group",
        located && "bg-primary/10 ring-2 ring-inset ring-primary",
      )}
      {...(located ? { "data-locate-row": "" } : {})}
    >
      <TableCell style={padding ? { paddingLeft: padding } : undefined}>
        {badge && (
          <Badge variant="outline" className={cn("mr-2 uppercase", KIND_BADGE_COLOR[item.kind])}>
            {item.kind}
          </Badge>
        )}
        <span className={cn("mr-2 font-semibold", !item.valid && "text-destructive")}>
          {item.name}
        </span>
        {ctx.lockedSkills.has(item.id) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Badge variant="outline" className="mr-2 gap-1 text-muted-foreground">
                <RefreshCw className="size-3" />
                skills.sh
              </Badge>
            </TooltipTrigger>
            <TooltipContent>Installed via skills.sh — update from the row menu</TooltipContent>
          </Tooltip>
        )}
        {!badge && <code className="font-mono text-[11px] text-muted-foreground">{item.relativePath}</code>}
        <RowActions item={item} ctx={ctx} />
      </TableCell>
      <TableCell>{item.sourceLabel}</TableCell>
      <ToolCells
        item={item}
        tools={ctx.tools}
        adapterMap={ctx.adapterMap}
        currentMap={ctx.currentMap}
        desired={ctx.desired}
        ownership={ctx.ownership}
        onToggle={ctx.onToggle}
        readOnly={ctx.readOnly}
        readOnlyItemIds={ctx.readOnlyItemIds}
      />
    </TableRow>
  );
}

function renderFlat(items: CapabilityItem[], ctx: BodyContext): ReactNode {
  return KIND_ORDER.map((kind) => {
    const rows = items.filter((it) => it.kind === kind);
    if (rows.length === 0) return null;
    return (
      <Fragment key={kind}>
        <TableRow className="bg-secondary/60 hover:bg-secondary/60">
          <TableCell className="text-xs font-bold uppercase tracking-[0.06em] text-muted-foreground">
            {KIND_LABEL[kind]} <span className="ml-1.5 text-primary">{rows.length}</span>
          </TableCell>
          <TableCell />
          <AggregateCells items={rows} ctx={ctx} />
        </TableRow>
        {rows.map((item) => leafRow(item, ctx))}
      </Fragment>
    );
  });
}

interface TreeNode {
  name: string;
  path: string;
  children: Map<string, TreeNode>;
  item?: CapabilityItem;
}

function buildTree(items: CapabilityItem[]): TreeNode {
  const root: TreeNode = { name: "", path: "", children: new Map() };
  for (const item of items) {
    const parts = item.relativePath.split("/").filter(Boolean);
    let node = root;
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      let child = node.children.get(part);
      if (!child) {
        child = { name: part, path: acc, children: new Map() };
        node.children.set(part, child);
      }
      node = child;
    }
    node.item = item;
  }
  return root;
}

function collectFolderPaths(node: TreeNode, out: string[] = []): string[] {
  for (const child of node.children.values()) {
    if (child.children.size > 0) {
      out.push(child.path);
      collectFolderPaths(child, out);
    }
  }
  return out;
}

function leavesUnder(node: TreeNode, out: CapabilityItem[] = []): CapabilityItem[] {
  if (node.item && node.children.size === 0) out.push(node.item);
  for (const child of node.children.values()) leavesUnder(child, out);
  return out;
}

function renderNodes(
  nodes: TreeNode[],
  depth: number,
  ctx: BodyContext,
  collapsed: ReadonlySet<string>,
  onToggleDir: (path: string) => void,
): ReactNode[] {
  const sorted = [...nodes].sort((a, b) => {
    const af = a.children.size > 0;
    const bf = b.children.size > 0;
    if (af !== bf) return af ? -1 : 1; // folders before leaves
    return a.name.localeCompare(b.name);
  });

  const out: ReactNode[] = [];
  for (const node of sorted) {
    const pad = depth * 16 + 12;
    if (node.children.size > 0) {
      const isCollapsed = collapsed.has(node.path);
      const leaves = leavesUnder(node);
      out.push(
        <TableRow key={`d:${node.path}`} className="bg-secondary/40 hover:bg-secondary/40">
          <TableCell style={{ paddingLeft: pad }}>
            <button
              className="mr-1 inline-flex size-4 cursor-pointer items-center justify-center align-middle text-muted-foreground hover:text-foreground"
              onClick={() => onToggleDir(node.path)}
              aria-label={isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`}
            >
              {isCollapsed ? <ChevronRight className="size-3.5" /> : <ChevronDown className="size-3.5" />}
            </button>
            <span className="font-semibold">{node.name}</span>
            <span className="ml-1.5 text-primary">{leaves.length}</span>
          </TableCell>
          <TableCell />
          <AggregateCells items={leaves} ctx={ctx} />
        </TableRow>,
      );
      if (!isCollapsed) {
        out.push(
          ...renderNodes([...node.children.values()], depth + 1, ctx, collapsed, onToggleDir),
        );
      }
    } else if (node.item) {
      out.push(leafRow(node.item, ctx, pad, true));
    }
  }
  return out;
}
