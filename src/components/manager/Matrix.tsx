// Capability matrix with two interchangeable layouts:
//   - flat: capabilities grouped by kind (Skills / Agents / Rules / Hooks)
//   - tree: capabilities nested by their source-relative folder path
// A shared search box filters both views. Group rows (kinds in flat, folders in
// tree) carry batch toggles that flip every capability beneath them per tool.

import { Fragment, useMemo, useState, type ReactNode } from "react";
import { Check, Minus, MoreHorizontal } from "lucide-react";
import { toast } from "sonner";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  Settings,
  ToolCapabilityState,
  ToolId,
} from "@/types";
import { openPath, revealPath } from "@/ipc";
import { key, KIND_LABEL, KIND_ORDER, messageOf, type ToolDef, type View } from "@/shared";
import { useManagerStore } from "@/state/manager";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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

const KIND_BADGE_COLOR: Record<CapabilityKind, string> = {
  skill: "text-primary",
  agent: "text-kind-agent",
  rule: "text-success",
  hook: "text-warning",
};

export function Matrix() {
  const data = useManagerStore((s) => s.data);
  const tools = useManagerStore((s) => s.tools);
  const currentMap = useManagerStore((s) => s.currentMap);
  const desired = useManagerStore((s) => s.desired);
  const onToggle = useManagerStore((s) => s.toggle);
  const onToggleMany = useManagerStore((s) => s.toggleMany);

  const [view, setView] = useState<View>("flat");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const [kind, setKind] = useState<KindFilter>("all");

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

  const filtered = useMemo(
    () => filterItems(items, query, source, kind),
    [items, query, source, kind],
  );
  const root = useMemo(() => buildTree(filtered), [filtered]);
  const folderPaths = useMemo(() => collectFolderPaths(root), [root]);

  if (!data || items.length === 0) {
    return (
      <Alert>
        <AlertDescription>No capabilities found in the configured sources.</AlertDescription>
      </Alert>
    );
  }

  const ctx: BodyContext = {
    tools,
    adapterMap,
    currentMap,
    desired,
    onToggle,
    onToggleMany,
    settings: data.settings,
  };
  const effectiveCollapsed = query.trim() ? EMPTY_COLLAPSE : collapsed;

  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex items-center gap-2.5">
        <ToggleGroup
          type="single"
          value={view}
          onValueChange={(v) => v && setView(v as View)}
          variant="outline"
        >
          <ToggleGroupItem value="flat">Flat</ToggleGroupItem>
          <ToggleGroupItem value="tree">Tree</ToggleGroupItem>
        </ToggleGroup>
        <Input
          type="search"
          className="flex-1"
          placeholder="Search by name, path, or source…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Select value={kind} onValueChange={(v) => setKind(v as KindFilter)}>
          <SelectTrigger className="w-[130px]" title="Filter by type">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All types</SelectItem>
            <SelectItem value="skill">Skills</SelectItem>
            <SelectItem value="agent">Agents</SelectItem>
            <SelectItem value="rule">Rules</SelectItem>
            <SelectItem value="hook">Hooks</SelectItem>
          </SelectContent>
        </Select>
        {sources.length > 1 && (
          <Select value={source || "all"} onValueChange={(v) => setSource(v === "all" ? "" : v)}>
            <SelectTrigger className="w-[150px]" title="Filter by source">
              <SelectValue />
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
        {view === "tree" && (
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={() => setCollapsed(new Set())}>
              Expand all
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setCollapsed(new Set(folderPaths))}>
              Collapse all
            </Button>
          </div>
        )}
      </div>
      {filtered.length === 0 ? (
        <Alert>
          <AlertDescription>No capabilities match “{query.trim()}”.</AlertDescription>
        </Alert>
      ) : (
        <div className="overflow-hidden rounded-xl border bg-card">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Capability</TableHead>
                <TableHead className="w-32">Source</TableHead>
                {tools.map((t) => {
                  const adapter = adapterMap.get(t.id);
                  const off = adapter && !adapter.available;
                  return (
                    <TableHead
                      key={t.id}
                      className="w-[120px] text-center"
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
                : renderNodes([...root.children.values()], 0, ctx, effectiveCollapsed, (path) =>
                    setCollapsed((prev) => {
                      const next = new Set(prev);
                      if (next.has(path)) next.delete(path);
                      else next.add(path);
                      return next;
                    }),
                  )}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  );
}

interface BodyContext {
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: Record<string, boolean>;
  onToggle: (tool: ToolId, itemId: string) => void;
  onToggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  settings: Settings;
}

type KindFilter = "all" | CapabilityKind;

function filterItems(
  items: CapabilityItem[],
  query: string,
  source: string,
  kind: KindFilter,
): CapabilityItem[] {
  const q = query.trim().toLowerCase();
  if (!q && !source && kind === "all") return items;
  return items.filter((it) => {
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
        const togglable = items.filter((it) => ctx.currentMap.has(key(t.id, it.id)));
        if (togglable.length === 0) {
          return (
            <TableCell key={t.id} className="text-center">
              <span className="text-muted-foreground/50">—</span>
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

// Editor app name for the opener `openWith` arg. Mirrors agentic-core
// `EditorPref::app_name` so "Open original" honors the Config setting.
function editorApp(settings: Settings): string | undefined {
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

// The original file to open: the marker file inside a skill/hook folder, or
// the capability file itself for agents/rules.
function originalFile(item: CapabilityItem): string {
  if (item.kind === "skill") return `${item.sourcePath}/SKILL.md`;
  if (item.kind === "hook") return `${item.sourcePath}/hook.json`;
  return item.sourcePath;
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

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          className="ml-2 align-middle opacity-0 transition-opacity group-hover:opacity-100 data-[state=open]:opacity-100"
          title="More actions"
          aria-label="More actions"
        >
          <MoreHorizontal className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
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
  return (
    <TableRow key={`l:${item.id}`} className="group">
      <TableCell style={padding ? { paddingLeft: padding } : undefined}>
        {badge && (
          <Badge variant="outline" className={cn("mr-2 uppercase", KIND_BADGE_COLOR[item.kind])}>
            {item.kind}
          </Badge>
        )}
        <span className={cn("mr-2 font-semibold", !item.valid && "text-destructive")}>
          {item.name}
        </span>
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
        onToggle={ctx.onToggle}
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
              className="mr-1 w-4 cursor-pointer text-[11px] text-muted-foreground"
              onClick={() => onToggleDir(node.path)}
            >
              {isCollapsed ? "▸" : "▾"}
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
