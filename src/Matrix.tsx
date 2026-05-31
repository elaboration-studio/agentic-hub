// Capability matrix with two interchangeable layouts:
//   - flat: capabilities grouped by kind (Skills / Agents / Rules / Hooks)
//   - tree: capabilities nested by their source-relative folder path
// A shared search box filters both views. Group rows (kinds in flat, folders in
// tree) carry batch toggles that flip every capability beneath them per tool.

import { Fragment, useMemo, useState, type ReactNode } from "react";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  ToolCapabilityState,
  ToolId,
} from "./types";
import type { DesiredMap } from "./ipc";
import {
  Banner,
  key,
  KIND_LABEL,
  KIND_ORDER,
  ToolCells,
  type ToolDef,
  type View,
} from "./shared";

interface MatrixProps {
  items: CapabilityItem[];
  tools: ToolDef[];
  currentMap: Map<string, ToolCapabilityState>;
  adapterStatuses: AdapterStatus[];
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
  onToggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
}

const EMPTY_COLLAPSE: ReadonlySet<string> = new Set();

export function Matrix(props: MatrixProps) {
  const [view, setView] = useState<View>("flat");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const [kind, setKind] = useState<KindFilter>("all");

  const adapterMap = useMemo(() => {
    const m = new Map<ToolId, AdapterStatus>();
    for (const a of props.adapterStatuses) m.set(a.tool, a);
    return m;
  }, [props.adapterStatuses]);

  // Distinct sources present in the scan, in first-seen order.
  const sources = useMemo(() => {
    const seen = new Map<string, string>();
    for (const it of props.items) if (!seen.has(it.sourceId)) seen.set(it.sourceId, it.sourceLabel);
    return [...seen].map(([id, label]) => ({ id, label }));
  }, [props.items]);

  const filtered = useMemo(
    () => filterItems(props.items, query, source, kind),
    [props.items, query, source, kind],
  );
  const root = useMemo(() => buildTree(filtered), [filtered]);
  const folderPaths = useMemo(() => collectFolderPaths(root), [root]);

  if (props.items.length === 0) {
    return <Banner tone="muted">No capabilities found in the configured sources.</Banner>;
  }

  const colCount = 2 + props.tools.length;
  const ctx: BodyContext = {
    tools: props.tools,
    adapterMap,
    currentMap: props.currentMap,
    desired: props.desired,
    onToggle: props.onToggle,
    onToggleMany: props.onToggleMany,
    colCount,
  };
  // While searching, ignore the manual collapse set so every match is visible.
  const effectiveCollapsed = query.trim() ? EMPTY_COLLAPSE : collapsed;

  return (
    <section className="matrix">
      <div className="matrix-toolbar">
        <div className="view-toggle" role="tablist">
          <button
            className={`view-tab${view === "flat" ? " active" : ""}`}
            onClick={() => setView("flat")}
          >
            Flat
          </button>
          <button
            className={`view-tab${view === "tree" ? " active" : ""}`}
            onClick={() => setView("tree")}
          >
            Tree
          </button>
        </div>
        <input
          className="matrix-search"
          type="search"
          placeholder="Search by name, path, or source…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select
          className="suite-tool kind-filter"
          value={kind}
          onChange={(e) => setKind(e.target.value as KindFilter)}
          title="Filter by type"
        >
          <option value="all">All types</option>
          <option value="skill">Skills</option>
          <option value="agent">Agents</option>
          <option value="rule">Rules</option>
          <option value="hook">Hooks</option>
        </select>
        {sources.length > 1 && (
          <select
            className="suite-tool source-filter"
            value={source}
            onChange={(e) => setSource(e.target.value)}
            title="Filter by source"
          >
            <option value="">All sources</option>
            {sources.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        )}
        {view === "tree" && (
          <div className="tree-controls">
            <button className="btn-ghost" onClick={() => setCollapsed(new Set())}>
              Expand all
            </button>
            <button className="btn-ghost" onClick={() => setCollapsed(new Set(folderPaths))}>
              Collapse all
            </button>
          </div>
        )}
      </div>
      {filtered.length === 0 ? (
        <Banner tone="muted">No capabilities match “{query.trim()}”.</Banner>
      ) : (
        <table>
          <thead>
            <tr>
              <th className="col-cap">Capability</th>
              <th className="col-src">Source</th>
              {props.tools.map((t) => {
                const adapter = adapterMap.get(t.id);
                const off = adapter && !adapter.available;
                return (
                  <th key={t.id} className="col-tool" title={adapter?.unavailableReason ?? ""}>
                    {t.label}
                    {off && <span className="tool-off">off</span>}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
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
          </tbody>
        </table>
      )}
    </section>
  );
}

interface BodyContext {
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
  onToggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  colCount: number;
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
            <td key={t.id} className="cell">
              <span className="dash">—</span>
            </td>
          );
        }
        const onCount = togglable.filter((it) => ctx.desired[key(t.id, it.id)]).length;
        const allOn = onCount === togglable.length;
        const mixed = onCount > 0 && !allOn;
        return (
          <td key={t.id} className="cell">
            <button
              className={`toggle batch${allOn ? " on" : ""}${mixed ? " mixed" : ""}`}
              title={`${onCount}/${togglable.length} on — click to ${allOn ? "disable" : "enable"} all`}
              onClick={() =>
                ctx.onToggleMany(
                  t.id,
                  togglable.map((it) => it.id),
                  !allOn,
                )
              }
            >
              {allOn ? "✓" : mixed ? "–" : ""}
            </button>
          </td>
        );
      })}
    </>
  );
}

function leafRow(item: CapabilityItem, ctx: BodyContext, padding?: number, badge?: boolean) {
  return (
    <tr key={`l:${item.id}`} className={item.valid ? "" : "invalid"}>
      <td className="col-cap" style={padding ? { paddingLeft: padding } : undefined}>
        {badge && <span className={`kind-badge kind-${item.kind}`}>{item.kind}</span>}
        <span className="cap-name">{item.name}</span>
        {!badge && <code className="cap-rel">{item.relativePath}</code>}
      </td>
      <td className="col-src">{item.sourceLabel}</td>
      <ToolCells
        item={item}
        tools={ctx.tools}
        adapterMap={ctx.adapterMap}
        currentMap={ctx.currentMap}
        desired={ctx.desired}
        onToggle={ctx.onToggle}
      />
    </tr>
  );
}

function renderFlat(items: CapabilityItem[], ctx: BodyContext): ReactNode {
  return KIND_ORDER.map((kind) => {
    const rows = items.filter((it) => it.kind === kind);
    if (rows.length === 0) return null;
    return (
      <Fragment key={kind}>
        <tr className="kind-row">
          <td className="col-cap">
            {KIND_LABEL[kind]} <span className="kind-count">{rows.length}</span>
          </td>
          <td className="col-src" />
          <AggregateCells items={rows} ctx={ctx} />
        </tr>
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
        <tr key={`d:${node.path}`} className="tree-dir-row">
          <td className="col-cap" style={{ paddingLeft: pad }}>
            <button className="tree-toggle" onClick={() => onToggleDir(node.path)}>
              {isCollapsed ? "▸" : "▾"}
            </button>
            <span className="tree-dir">{node.name}</span>
            <span className="kind-count">{leaves.length}</span>
          </td>
          <td className="col-src" />
          <AggregateCells items={leaves} ctx={ctx} />
        </tr>,
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
