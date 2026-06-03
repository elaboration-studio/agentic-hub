// Suite Manager route: a two-pane CRUD editor for capability suites. Left pane
// lists suites; right pane edits the selected/new suite with a checkbox
// capability tree (search + kind filter; folder/kind nodes batch-toggle their
// descendants). Suite data + draft live in the suites store; view filters are
// local; capability source comes from the manager store.

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { onSuiteStoreChanged } from "@/ipc";
import type { CapabilityItem, CapabilityKind, ToolId } from "@/types";
import { type ToolDef } from "@/shared";
import { useManagerStore } from "@/state/manager";
import { useSuitesStore } from "@/state/suites";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";

const SUITE_KINDS: CapabilityKind[] = ["skill", "agent", "rule"];
const KIND_LABELS: Record<string, string> = { skill: "Skills", agent: "Agents", rule: "Rules" };
const sectionTitle = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

type KindFilter = "all" | CapabilityKind;

interface SNode {
  id: string;
  type: "kind" | "folder" | "item";
  label: string;
  description: string;
  children: SNode[];
  itemIds: string[];
  item?: CapabilityItem;
}

const EMPTY: ReadonlySet<string> = new Set();

interface Confirm {
  title: string;
  body: string;
  onConfirm: () => void;
}

export function SuitesPage() {
  const items = useManagerStore((s) => s.data?.items ?? []);
  const tools = useManagerStore((s) => s.tools);
  const adapterStatuses = useManagerStore((s) => s.data?.result.adapterStatuses ?? []);

  const suites = useSuitesStore((s) => s.suites);
  const selectedId = useSuitesStore((s) => s.selectedId);
  const isCreating = useSuitesStore((s) => s.isCreating);
  const draft = useSuitesStore((s) => s.draft);
  const busy = useSuitesStore((s) => s.busy);
  const reload = useSuitesStore((s) => s.reload);
  const startCreate = useSuitesStore((s) => s.startCreate);
  const selectSuite = useSuitesStore((s) => s.selectSuite);
  const cancelEdit = useSuitesStore((s) => s.cancelEdit);
  const setDraft = useSuitesStore((s) => s.setDraft);
  const setCapabilities = useSuitesStore((s) => s.setCapabilities);
  const save = useSuitesStore((s) => s.save);
  const removeSuite = useSuitesStore((s) => s.remove);
  const applySelected = useSuitesStore((s) => s.applySelected);
  const pruneSelection = useSuitesStore((s) => s.pruneSelection);

  const [capSearch, setCapSearch] = useState("");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const availableTools = useMemo(
    () => tools.filter((t: ToolDef) => adapterStatuses.find((a) => a.tool === t.id)?.available),
    [tools, adapterStatuses],
  );
  const [applyTool, setApplyTool] = useState<ToolId>(availableTools[0]?.id ?? "codex");

  useEffect(() => {
    setApplyTool((cur) =>
      availableTools.some((t) => t.id === cur) ? cur : (availableTools[0]?.id ?? cur),
    );
  }, [availableTools]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const unlisten = onSuiteStoreChanged(() => void reload());
    return () => void unlisten.then((fn) => fn());
  }, [reload]);

  useEffect(() => {
    pruneSelection();
  }, [suites, pruneSelection]);

  const included = useMemo(() => new Set(draft?.capabilities ?? []), [draft]);

  const visibleItems = useMemo(() => {
    const q = capSearch.trim().toLowerCase();
    return items.filter((it) => {
      if (!SUITE_KINDS.includes(it.kind)) return false;
      if (kindFilter !== "all" && it.kind !== kindFilter) return false;
      if (!q) return true;
      return it.name.toLowerCase().includes(q) || it.relativePath.toLowerCase().includes(q);
    });
  }, [items, capSearch, kindFilter]);

  const tree = useMemo(() => buildTree(visibleItems), [visibleItems]);

  const toggleDir = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const onApply = useCallback(() => {
    const suite = suites.find((s) => s.id === selectedId);
    if (!suite || suite.capabilities.length === 0) {
      setConfirm({
        title: "Apply empty suite?",
        body: "This suite is empty. Applying disables every capability for the tool. Continue?",
        onConfirm: () => void applySelected(applyTool),
      });
      return;
    }
    void applySelected(applyTool);
  }, [suites, selectedId, applyTool, applySelected]);

  const onDelete = useCallback(() => {
    const suite = suites.find((s) => s.id === selectedId);
    setConfirm({
      title: `Delete suite "${suite?.name ?? selectedId}"?`,
      body: "This cannot be undone.",
      onConfirm: () => void removeSuite(),
    });
  }, [suites, selectedId, removeSuite]);

  const effectiveCollapsed = capSearch.trim() ? EMPTY : collapsed;

  return (
    <div className="grid items-start gap-[18px] md:grid-cols-[minmax(240px,1fr)_minmax(380px,2fr)]">
      <Card className="p-4">
        <CardHeader className="flex-row items-center justify-between p-0">
          <CardTitle className={sectionTitle}>Suites</CardTitle>
          <Button size="sm" onClick={startCreate} disabled={busy}>
            + New
          </Button>
        </CardHeader>
        <CardContent className="p-0">
          {suites.length === 0 && !isCreating ? (
            <div className="py-6 text-center text-muted-foreground">
              No suites yet. Click “+ New” to create one.
            </div>
          ) : (
            <ul className="flex flex-col gap-1">
              {suites.map((s) => (
                <li
                  key={s.id}
                  className={cn(
                    "flex cursor-pointer items-center justify-between gap-2 rounded-lg border border-transparent px-2.5 py-2 hover:bg-secondary",
                    s.id === selectedId && !isCreating && "border-primary bg-secondary",
                  )}
                  onClick={() => selectSuite(s.id)}
                >
                  <span className="truncate font-semibold">{s.name}</span>
                  <span className="whitespace-nowrap text-[11px] text-muted-foreground">
                    {s.capabilities.length} items
                  </span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card className="p-4">
        <CardHeader className="flex-row items-center justify-between p-0">
          <CardTitle className={sectionTitle}>
            {isCreating ? "New Suite" : draft ? "Edit Suite" : "Editor"}
          </CardTitle>
          {draft && (
            <span className="text-xs text-muted-foreground">{draft.capabilities.length} selected</span>
          )}
        </CardHeader>
        <CardContent className="flex flex-col gap-3.5 p-0">
          {selectedId && !isCreating && (
            <div className="flex flex-wrap items-center gap-2.5 border-b pb-3.5">
              <span className={sectionTitle}>Apply</span>
              <Select
                value={applyTool}
                onValueChange={(v) => setApplyTool(v as ToolId)}
                disabled={busy || availableTools.length === 0}
              >
                <SelectTrigger className="w-[160px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {availableTools.map((t) => (
                    <SelectItem key={t.id} value={t.id}>
                      {t.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button onClick={onApply} disabled={busy || availableTools.length === 0}>
                Apply Suite
              </Button>
            </div>
          )}
          {!draft ? (
            <div className="py-6 text-center text-muted-foreground">
              Select a suite to edit, or create a new one.
            </div>
          ) : (
            <div className="grid gap-3.5">
              <div className="grid gap-1">
                <Label className={sectionTitle}>Name</Label>
                <Input
                  value={draft.name}
                  placeholder="Suite name"
                  onChange={(e) => setDraft({ name: e.target.value })}
                />
              </div>
              <div className="grid gap-1">
                <Label className={sectionTitle}>Description</Label>
                <Input
                  value={draft.description}
                  placeholder="Optional description"
                  onChange={(e) => setDraft({ description: e.target.value })}
                />
              </div>
              <div className="grid gap-1">
                <Label className={sectionTitle}>Capabilities</Label>
                <div className="grid grid-cols-[1fr_140px] gap-2">
                  <Input
                    type="search"
                    placeholder="Search capabilities"
                    value={capSearch}
                    onChange={(e) => setCapSearch(e.target.value)}
                  />
                  <Select value={kindFilter} onValueChange={(v) => setKindFilter(v as KindFilter)}>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All kinds</SelectItem>
                      <SelectItem value="skill">Skills</SelectItem>
                      <SelectItem value="agent">Agents</SelectItem>
                      <SelectItem value="rule">Rules</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="max-h-[48vh] overflow-auto rounded-lg border bg-background p-1">
                  {visibleItems.length === 0 ? (
                    <div className="py-6 text-center text-muted-foreground">
                      No capabilities match the current filters.
                    </div>
                  ) : (
                    renderNodes(tree, 0, {
                      included,
                      collapsed: effectiveCollapsed,
                      onToggleDir: toggleDir,
                      setCapabilities,
                    })
                  )}
                </div>
              </div>
              <div className="flex flex-wrap gap-2 pt-0.5">
                <Button onClick={() => void save(items)} disabled={busy || !draft.name.trim()}>
                  Save
                </Button>
                {!isCreating && (
                  <Button
                    variant="ghost"
                    className="text-destructive hover:text-destructive"
                    onClick={onDelete}
                    disabled={busy}
                  >
                    Delete
                  </Button>
                )}
                <Button variant="ghost" onClick={cancelEdit} disabled={busy}>
                  Cancel
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <AlertDialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirm?.title}</AlertDialogTitle>
            <AlertDialogDescription>{confirm?.body}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                confirm?.onConfirm();
                setConfirm(null);
              }}
            >
              Continue
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function buildTree(items: CapabilityItem[]): SNode[] {
  const roots: SNode[] = [];
  for (const kind of SUITE_KINDS) {
    const kindItems = items.filter((it) => it.kind === kind);
    if (kindItems.length === 0) continue;
    const kindNode: SNode = {
      id: `kind:${kind}`,
      type: "kind",
      label: KIND_LABELS[kind],
      description: `${kindItems.length} visible`,
      children: [],
      itemIds: kindItems.map((it) => it.id),
    };
    roots.push(kindNode);
    const folderMap = new Map<string, SNode>([["", kindNode]]);

    for (const item of kindItems) {
      const segments = item.relativePath.split("/").filter(Boolean).slice(0, -1);
      let parent = kindNode;
      let pathAcc = "";
      for (const seg of segments) {
        pathAcc = pathAcc ? `${pathAcc}/${seg}` : seg;
        let folder = folderMap.get(pathAcc);
        if (!folder) {
          folder = {
            id: `folder:${kind}:${pathAcc}`,
            type: "folder",
            label: seg,
            description: pathAcc,
            children: [],
            itemIds: [],
          };
          folderMap.set(pathAcc, folder);
          parent.children.push(folder);
        }
        folder.itemIds.push(item.id);
        parent = folder;
      }
      parent.children.push({
        id: `item:${item.id}`,
        type: "item",
        label: item.name,
        description: item.relativePath,
        children: [],
        itemIds: [item.id],
        item,
      });
    }
  }
  return roots;
}

interface TreeCtx {
  included: Set<string>;
  collapsed: ReadonlySet<string>;
  onToggleDir: (id: string) => void;
  setCapabilities: (ids: string[], on: boolean) => void;
}

function renderNodes(nodes: SNode[], depth: number, ctx: TreeCtx): ReactNode[] {
  const out: ReactNode[] = [];
  for (const node of nodes) {
    const total = node.itemIds.length;
    const includedCount = node.itemIds.filter((id) => ctx.included.has(id)).length;
    const allOn = total > 0 && includedCount === total;
    const indeterminate = includedCount > 0 && includedCount < total;
    const expandable = node.children.length > 0;
    const expanded = expandable && !ctx.collapsed.has(node.id);
    const isItem = node.type === "item";
    const itemIncluded = isItem && ctx.included.has(node.item!.id);
    const checkedState: boolean | "indeterminate" = isItem
      ? itemIncluded
      : indeterminate
        ? "indeterminate"
        : allOn;

    out.push(
      <div
        key={node.id}
        className="grid grid-cols-[18px_16px_minmax(0,1fr)_auto] items-center gap-2 rounded-md px-1.5 py-1 hover:bg-secondary"
        style={{ paddingLeft: depth * 16 + 6 }}
      >
        {expandable ? (
          <button
            className="size-[18px] cursor-pointer text-[11px] text-muted-foreground"
            onClick={() => ctx.onToggleDir(node.id)}
          >
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className="size-[18px]" />
        )}
        <Checkbox
          checked={checkedState}
          onCheckedChange={(c) => ctx.setCapabilities(node.itemIds, c === true)}
        />
        <div className="min-w-0">
          <div className="truncate">{node.label}</div>
          <div className="truncate text-[11px] text-muted-foreground">{node.description}</div>
        </div>
        <Badge
          variant="outline"
          className={cn("whitespace-nowrap", (allOn || itemIncluded) && "border-success/40 text-success")}
        >
          {isItem ? (itemIncluded ? "Included" : node.item!.kind) : `${includedCount}/${total}`}
        </Badge>
      </div>,
    );
    if (expanded) out.push(...renderNodes(node.children, depth + 1, ctx));
  }
  return out;
}
