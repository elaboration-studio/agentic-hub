// Capability matrix with two interchangeable layouts:
//   - flat: capabilities grouped by kind (Skills / Agents / Rules / Hooks)
//   - tree: capabilities nested by their source-relative folder path
// Both share the per-tool toggle cells from `shared.tsx`.

import { Fragment, useMemo, useState, type ReactNode } from "react";
import type {
  AdapterStatus,
  CapabilityItem,
  ToolCapabilityState,
  ToolId,
} from "./types";
import type { DesiredMap } from "./ipc";
import {
  Banner,
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
}

export function Matrix(props: MatrixProps) {
  const [view, setView] = useState<View>("flat");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const adapterMap = useMemo(() => {
    const m = new Map<ToolId, AdapterStatus>();
    for (const a of props.adapterStatuses) m.set(a.tool, a);
    return m;
  }, [props.adapterStatuses]);

  const root = useMemo(() => buildTree(props.items), [props.items]);
  const folderPaths = useMemo(() => collectFolderPaths(root), [root]);

  if (props.items.length === 0) {
    return <Banner tone="muted">No capabilities found in the configured sources.</Banner>;
  }

  const colCount = 2 + props.tools.length;
  const body: BodyContext = {
    tools: props.tools,
    adapterMap,
    currentMap: props.currentMap,
    desired: props.desired,
    onToggle: props.onToggle,
    colCount,
  };

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
            ? renderFlat(props.items, body)
            : renderNodes([...root.children.values()], 0, body, collapsed, (path) =>
                setCollapsed((prev) => {
                  const next = new Set(prev);
                  if (next.has(path)) next.delete(path);
                  else next.add(path);
                  return next;
                }),
              )}
        </tbody>
      </table>
    </section>
  );
}

interface BodyContext {
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
  colCount: number;
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
          <td colSpan={ctx.colCount}>
            {KIND_LABEL[kind]} <span className="kind-count">{rows.length}</span>
          </td>
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

function countLeaves(node: TreeNode): number {
  let n = node.item && node.children.size === 0 ? 1 : 0;
  for (const child of node.children.values()) n += countLeaves(child);
  return n;
}

function renderNodes(
  nodes: TreeNode[],
  depth: number,
  ctx: BodyContext,
  collapsed: Set<string>,
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
      out.push(
        <tr key={`d:${node.path}`} className="tree-dir-row">
          <td className="col-cap" colSpan={ctx.colCount} style={{ paddingLeft: pad }}>
            <button className="tree-toggle" onClick={() => onToggleDir(node.path)}>
              {isCollapsed ? "▸" : "▾"}
            </button>
            <span className="tree-dir">{node.name}</span>
            <span className="kind-count">{countLeaves(node)}</span>
          </td>
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
