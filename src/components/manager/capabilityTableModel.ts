import type { CapabilityItem } from "@/types";
import type { KindFilter } from "@/shared";

export interface CapabilityFilter {
  query: string;
  source: string;
  kind: KindFilter;
  enabledOnly: boolean;
  enabledItemIds: ReadonlySet<string>;
}

export interface CapabilityTreeNode {
  name: string;
  path: string;
  children: Map<string, CapabilityTreeNode>;
  item?: CapabilityItem;
}

export function filterCapabilityItems(
  items: CapabilityItem[],
  filter: CapabilityFilter,
): CapabilityItem[] {
  const query = filter.query.trim().toLowerCase();
  if (!query && !filter.source && filter.kind === "all" && !filter.enabledOnly) return items;
  return items.filter((item) => {
    if (filter.enabledOnly && !filter.enabledItemIds.has(item.id)) return false;
    if (filter.kind !== "all" && item.kind !== filter.kind) return false;
    if (filter.source && item.sourceId !== filter.source) return false;
    if (!query) return true;
    return (
      item.name.toLowerCase().includes(query) ||
      item.relativePath.toLowerCase().includes(query) ||
      item.sourceLabel.toLowerCase().includes(query)
    );
  });
}

export function buildCapabilityTree(items: CapabilityItem[]): CapabilityTreeNode {
  const root: CapabilityTreeNode = { name: "", path: "", children: new Map() };
  for (const item of items) {
    const parts = item.relativePath.split("/").filter(Boolean);
    let node = root;
    let path = "";
    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      let child = node.children.get(part);
      if (!child) {
        child = { name: part, path, children: new Map() };
        node.children.set(part, child);
      }
      node = child;
    }
    node.item = item;
  }
  return root;
}

export function collectCapabilityFolderPaths(
  node: CapabilityTreeNode,
  out: string[] = [],
): string[] {
  for (const child of node.children.values()) {
    if (child.children.size > 0) {
      out.push(child.path);
      collectCapabilityFolderPaths(child, out);
    }
  }
  return out;
}

export function leavesUnder(
  node: CapabilityTreeNode,
  out: CapabilityItem[] = [],
): CapabilityItem[] {
  if (node.item && node.children.size === 0) out.push(node.item);
  for (const child of node.children.values()) leavesUnder(child, out);
  return out;
}

export function ancestorPaths(relativePath: string): string[] {
  const parts = relativePath.split("/").filter(Boolean);
  const out: string[] = [];
  let path = "";
  for (let index = 0; index < parts.length - 1; index += 1) {
    path = path ? `${path}/${parts[index]}` : parts[index];
    out.push(path);
  }
  return out;
}
