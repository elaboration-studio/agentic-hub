import { Fragment, useEffect, useMemo, type ReactNode } from "react";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Clock,
  Database,
  FolderTree,
  List,
  RefreshCw,
  Search,
  Tag,
} from "lucide-react";
import type { CapabilityItem, CapabilityKind, ToolId, UsageStats } from "@/types";
import {
  KIND_LABEL,
  KIND_ORDER,
  TOOL_LABELS,
  type KindFilter,
  type UsageSort,
  type View,
  USAGE_SORT_LABEL,
} from "@/shared";
import { useManagerFiltersStore } from "@/state/managerFilters";
import { compareByUsageSort } from "@/state/managerSort";
import { formatLocalTimestamp } from "@/lib/format";
import { cn } from "@/lib/utils";
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
  ancestorPaths,
  buildCapabilityTree,
  collectCapabilityFolderPaths,
  filterCapabilityItems,
  leavesUnder,
  type CapabilityTreeNode,
} from "./capabilityTableModel";

const EMPTY_COLLAPSE: ReadonlySet<string> = new Set();
const STICKY_HEAD = "sticky top-14 z-20 bg-card";
// MCP lists after the projected kinds: suites can include it, but no tool home
// receives it (bundles only), so Manager cells render "—".
const TABLE_KINDS: CapabilityKind[] = [...KIND_ORDER, "mcp"];

const KIND_BADGE_COLOR: Record<CapabilityKind, string> = {
  skill: "text-primary",
  agent: "text-kind-agent",
  rule: "text-success",
  hook: "text-warning",
  command: "text-kind-command",
  mcp: "text-muted-foreground",
};

export interface CapabilityStateColumn {
  id: string;
  label: ReactNode;
  className?: string;
  title?: string;
  unavailable?: boolean;
}

interface CapabilityTableProps {
  items: CapabilityItem[];
  stateColumns: CapabilityStateColumn[];
  enabledItemIds: ReadonlySet<string>;
  usageStats: ReadonlyMap<string, UsageStats>;
  showEnabledOnly?: boolean;
  enabledOnlyLabel?: string;
  locateId?: string;
  onLocateConsumed?: () => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  renderStateCells: (item: CapabilityItem) => ReactNode;
  renderAggregateCells: (items: CapabilityItem[]) => ReactNode;
  renderRowActions?: (item: CapabilityItem) => ReactNode;
  renderItemMeta?: (item: CapabilityItem) => ReactNode;
}

export function CapabilityTable(props: CapabilityTableProps) {
  const view = useManagerFiltersStore((state) => state.view);
  const setView = useManagerFiltersStore((state) => state.setView);
  const collapsed = useManagerFiltersStore((state) => state.collapsed);
  const setCollapsed = useManagerFiltersStore((state) => state.setCollapsed);
  const toggleCollapsed = useManagerFiltersStore((state) => state.toggleCollapsed);
  const query = useManagerFiltersStore((state) => state.query);
  const setQuery = useManagerFiltersStore((state) => state.setQuery);
  const source = useManagerFiltersStore((state) => state.source);
  const setSource = useManagerFiltersStore((state) => state.setSource);
  const kind = useManagerFiltersStore((state) => state.kind);
  const setKind = useManagerFiltersStore((state) => state.setKind);
  const enabledOnly = useManagerFiltersStore((state) => state.enabledOnly);
  const setEnabledOnly = useManagerFiltersStore((state) => state.setEnabledOnly);
  const usageSort = useManagerFiltersStore((state) => state.usageSort);
  const setUsageSort = useManagerFiltersStore((state) => state.setUsageSort);

  const sources = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of props.items) {
      if (!seen.has(item.sourceId)) seen.set(item.sourceId, item.sourceLabel);
    }
    return [...seen].map(([id, label]) => ({ id, label }));
  }, [props.items]);

  const filtered = useMemo(
    () =>
      filterCapabilityItems(props.items, {
        query,
        source,
        kind,
        enabledOnly: !!props.showEnabledOnly && enabledOnly,
        enabledItemIds: props.enabledItemIds,
      }),
    [props.items, query, source, kind, enabledOnly, props.showEnabledOnly, props.enabledItemIds],
  );
  const root = useMemo(() => buildCapabilityTree(filtered), [filtered]);
  const folderPaths = useMemo(() => collectCapabilityFolderPaths(root), [root]);

  useEffect(() => {
    if (!props.locateId) return;
    const target = props.items.find((item) => item.id === props.locateId);
    if (!target) return;
    const ancestors = ancestorPaths(target.relativePath);
    const current = useManagerFiltersStore.getState().collapsed;
    if (ancestors.some((path) => current.has(path))) {
      setCollapsed(new Set([...current].filter((path) => !ancestors.includes(path))));
    }
    const scroll = window.setTimeout(() => {
      document
        .querySelector("[data-locate-row]")
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    }, 80);
    const clear = window.setTimeout(() => props.onLocateConsumed?.(), 2200);
    return () => {
      window.clearTimeout(scroll);
      window.clearTimeout(clear);
    };
  }, [props.locateId, props.items, props.onLocateConsumed, setCollapsed]);

  const context: RenderContext = { props, usageSort };
  const effectiveCollapsed = query.trim() ? EMPTY_COLLAPSE : collapsed;

  return (
    <section className="flex min-w-fit flex-col gap-2.5">
      <div className="sticky top-0 z-30 flex h-14 items-center gap-2.5 bg-background shadow-[0_-1.25rem_0_0_var(--background)]">
        <ToggleGroup
          type="single"
          value={view}
          onValueChange={(next) => next && setView(next as View)}
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
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="search"
            className="pl-9"
            placeholder="Search by name, path, or source…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <Select value={kind} onValueChange={(next) => setKind(next as KindFilter)}>
          <SelectTrigger className="w-[140px]" title="Filter by type">
            <span className="flex min-w-0 items-center gap-2"><Tag /><SelectValue /></span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All types</SelectItem>
            {TABLE_KINDS.map((entry) => <SelectItem key={entry} value={entry}>{KIND_LABEL[entry]}</SelectItem>)}
          </SelectContent>
        </Select>
        <Select value={usageSort} onValueChange={(next) => setUsageSort(next as UsageSort)}>
          <SelectTrigger className="w-[150px]" title="Sort by usage">
            <span className="flex min-w-0 items-center gap-2"><Clock /><SelectValue /></span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="lastUsed">{USAGE_SORT_LABEL.lastUsed}</SelectItem>
            <SelectItem value="usageCount">{USAGE_SORT_LABEL.usageCount}</SelectItem>
          </SelectContent>
        </Select>
        {sources.length > 1 && (
          <Select value={source || "all"} onValueChange={(next) => setSource(next === "all" ? "" : next)}>
            <SelectTrigger className="w-[160px]" title="Filter by source">
              <span className="flex min-w-0 items-center gap-2"><Database /><SelectValue /></span>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All sources</SelectItem>
              {sources.map((entry) => <SelectItem key={entry.id} value={entry.id}>{entry.label}</SelectItem>)}
            </SelectContent>
          </Select>
        )}
        {props.showEnabledOnly && (
          <Label className="flex shrink-0 items-center gap-2 text-muted-foreground">
            <Checkbox checked={enabledOnly} onCheckedChange={(value) => setEnabledOnly(value === true)} />
            {props.enabledOnlyLabel ?? "Enabled only"}
          </Label>
        )}
        {view === "tree" && (
          <div className="flex gap-1">
            <ToolbarButton label="Expand all folders" onClick={() => setCollapsed(new Set())}><ChevronsUpDown /></ToolbarButton>
            <ToolbarButton label="Collapse all folders" onClick={() => setCollapsed(new Set(folderPaths))}><ChevronsDownUp /></ToolbarButton>
          </div>
        )}
        {props.onRefresh && (
          <ToolbarButton label="Refresh resources and usage" onClick={props.onRefresh} disabled={props.refreshing}>
            <RefreshCw className={cn("size-4", props.refreshing && "animate-spin")} />
          </ToolbarButton>
        )}
      </div>
      {filtered.length === 0 ? (
        <Alert><AlertDescription>No capabilities match “{query.trim()}”.</AlertDescription></Alert>
      ) : (
        <Table containerClassName="overflow-x-visible overflow-y-visible rounded-xl border bg-card">
          <TableHeader>
            <TableRow className="hover:bg-card">
              <TableHead className={STICKY_HEAD}>Capability</TableHead>
              <TableHead className={cn("w-32", STICKY_HEAD)}>Source</TableHead>
              <TableHead className={cn("w-24 text-right", STICKY_HEAD)}>Usage</TableHead>
              {props.stateColumns.map((column) => (
                <TableHead
                  key={column.id}
                  className={cn("w-[120px] text-center", STICKY_HEAD, column.className)}
                  title={column.title}
                >
                  {column.label}
                  {column.unavailable && <span className="ml-1.5 rounded border px-1 text-[9px] uppercase text-muted-foreground">off</span>}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {view === "flat"
              ? renderFlat(filtered, context)
              : renderTree([...root.children.values()], 0, context, effectiveCollapsed, toggleCollapsed)}
          </TableBody>
        </Table>
      )}
    </section>
  );
}

function ToolbarButton(props: { label: string; onClick: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={props.label} onClick={props.onClick} disabled={props.disabled}>
          {props.children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{props.label}</TooltipContent>
    </Tooltip>
  );
}

function UsageCell({ item, stats }: { item: CapabilityItem; stats?: UsageStats }) {
  const countable = item.kind === "skill" || item.kind === "command" || item.kind === "agent";
  if (!countable || !stats || stats.executionCount === 0) {
    return <TableCell className="text-right"><span className="text-muted-foreground/50">—</span></TableCell>;
  }
  return (
    <TableCell className="text-right">
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            className="rounded px-1.5 py-0.5 font-mono text-xs tabular-nums text-primary hover:bg-primary/10"
            aria-label={`${item.name} usage count`}
          >
            {stats.executionCount}
          </button>
        </TooltipTrigger>
        <TooltipContent align="end" className="max-w-[260px]">
          <div className="flex min-w-[180px] flex-col gap-1">
            <div className="flex items-center justify-between gap-3"><span className="font-semibold">Total</span><span className="font-mono tabular-nums">{stats.executionCount}</span></div>
            {stats.toolBuckets.map((bucket) => <div key={bucket.sourceTool} className="flex items-center justify-between gap-3 text-xs"><span>{bucket.sourceTool === "agentic-hub" ? "Palette" : (TOOL_LABELS[bucket.sourceTool as ToolId] ?? bucket.sourceTool)}</span><span className="font-mono tabular-nums">{bucket.executionCount}</span></div>)}
            {stats.lastUsedAt && <div className="border-t pt-1 text-[11px] text-muted-foreground">Last used {formatLocalTimestamp(stats.lastUsedAt)}</div>}
          </div>
        </TooltipContent>
      </Tooltip>
    </TableCell>
  );
}

interface RenderContext {
  props: CapabilityTableProps;
  usageSort: UsageSort;
}

function leafRow(item: CapabilityItem, context: RenderContext, padding?: number, badge?: boolean) {
  const { props } = context;
  const located = !!props.locateId && item.id === props.locateId;
  return (
    <TableRow key={`l:${item.id}`} className={cn("group", located && "bg-primary/10 ring-2 ring-inset ring-primary")} {...(located ? { "data-locate-row": "" } : {})}>
      <TableCell style={padding ? { paddingLeft: padding } : undefined}>
        {badge && <Badge variant="outline" className={cn("mr-2 uppercase", KIND_BADGE_COLOR[item.kind])}>{item.kind}</Badge>}
        <span className={cn("mr-2 font-semibold", !item.valid && "text-destructive")}>{item.name}</span>
        {!badge && <code className="font-mono text-[11px] text-muted-foreground">{item.relativePath}</code>}
        {props.renderItemMeta?.(item)}
        {props.renderRowActions?.(item)}
      </TableCell>
      <TableCell>{item.sourceLabel}</TableCell>
      <UsageCell item={item} stats={props.usageStats.get(item.id)} />
      {props.renderStateCells(item)}
    </TableRow>
  );
}

function renderFlat(items: CapabilityItem[], context: RenderContext): ReactNode {
  return TABLE_KINDS.map((kind) => {
    const rows = items
      .filter((item) => item.kind === kind)
      .sort((left, right) => compareByUsageSort(left, right, context.usageSort, context.props.usageStats));
    if (rows.length === 0) return null;
    return (
      <Fragment key={kind}>
        <TableRow className="bg-secondary/60 hover:bg-secondary/60">
          <TableCell className="text-xs font-bold uppercase tracking-[0.06em] text-muted-foreground">{KIND_LABEL[kind]} <span className="ml-1.5 text-primary">{rows.length}</span></TableCell>
          <TableCell /><TableCell />{context.props.renderAggregateCells(rows)}
        </TableRow>
        {rows.map((item) => leafRow(item, context))}
      </Fragment>
    );
  });
}

function renderTree(
  nodes: CapabilityTreeNode[],
  depth: number,
  context: RenderContext,
  collapsed: ReadonlySet<string>,
  toggle: (path: string) => void,
): ReactNode[] {
  const sorted = [...nodes].sort((left, right) => {
    const leftFolder = left.children.size > 0;
    const rightFolder = right.children.size > 0;
    if (leftFolder !== rightFolder) return leftFolder ? -1 : 1;
    if (leftFolder && rightFolder) return left.name.localeCompare(right.name);
    if (left.item && right.item) return compareByUsageSort(left.item, right.item, context.usageSort, context.props.usageStats);
    return left.name.localeCompare(right.name);
  });
  const rows: ReactNode[] = [];
  for (const node of sorted) {
    const padding = depth * 16 + 12;
    if (node.children.size > 0) {
      const isCollapsed = collapsed.has(node.path);
      const leaves = leavesUnder(node);
      rows.push(
        <TableRow key={`d:${node.path}`} className="bg-secondary/40 hover:bg-secondary/40">
          <TableCell style={{ paddingLeft: padding }}>
            <button className="mr-1 inline-flex size-4 cursor-pointer items-center justify-center align-middle text-muted-foreground hover:text-foreground" onClick={() => toggle(node.path)} aria-label={isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`}>
              {isCollapsed ? <ChevronRight className="size-3.5" /> : <ChevronDown className="size-3.5" />}
            </button>
            <span className="font-semibold">{node.name}</span><span className="ml-1.5 text-primary">{leaves.length}</span>
          </TableCell>
          <TableCell /><TableCell />{context.props.renderAggregateCells(leaves)}
        </TableRow>,
      );
      if (!isCollapsed) rows.push(...renderTree([...node.children.values()], depth + 1, context, collapsed, toggle));
    } else if (node.item) {
      rows.push(leafRow(node.item, context, padding, true));
    }
  }
  return rows;
}
