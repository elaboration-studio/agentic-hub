// Suite Manager route: a two-pane CRUD editor for capability suites, ported
// from the VS Code extension's Suite Manager. Left pane lists suites; right
// pane edits the selected/new suite with a checkbox capability tree (search +
// kind filter, folder/kind nodes batch-toggle their descendants).

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  applySuite,
  createSuite,
  deleteSuite,
  listSuites,
  onSuiteStoreChanged,
  updateSuite,
} from "./ipc";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  SuiteDefinition,
  ToolId,
} from "./types";
import { messageOf, type ToolDef } from "./shared";

// The extension scopes suite editing to these kinds (hooks are excluded).
const SUITE_KINDS: CapabilityKind[] = ["skill", "agent", "rule"];
const KIND_LABELS: Record<string, string> = { skill: "Skills", agent: "Agents", rule: "Rules" };

type KindFilter = "all" | CapabilityKind;

interface Draft {
  name: string;
  description: string;
  capabilities: string[];
}

interface SNode {
  id: string;
  type: "kind" | "folder" | "item";
  label: string;
  description: string;
  children: SNode[];
  itemIds: string[];
  item?: CapabilityItem;
}

export function SuitesPage(props: {
  items: CapabilityItem[];
  tools: ToolDef[];
  adapterStatuses: AdapterStatus[];
  onError: (msg: string) => void;
}) {
  const [suites, setSuites] = useState<SuiteDefinition[]>([]);
  const [selectedId, setSelectedId] = useState<string | undefined>();
  const [isCreating, setIsCreating] = useState(false);
  const [draft, setDraft] = useState<Draft | undefined>();
  const [capSearch, setCapSearch] = useState("");
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const availableTools = useMemo(
    () => props.tools.filter((t) => props.adapterStatuses.find((a) => a.tool === t.id)?.available),
    [props.tools, props.adapterStatuses],
  );
  const [applyTool, setApplyTool] = useState<ToolId>(availableTools[0]?.id ?? "codex");
  const [applyMsg, setApplyMsg] = useState("");

  useEffect(() => {
    setApplyTool((cur) =>
      availableTools.some((t) => t.id === cur) ? cur : (availableTools[0]?.id ?? cur),
    );
  }, [availableTools]);

  const reload = useCallback(async () => {
    try {
      setSuites(await listSuites());
    } catch (e) {
      props.onError(messageOf(e));
    }
  }, [props]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const unlisten = onSuiteStoreChanged(() => void reload());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [reload]);

  // Drop selection if the suite disappeared (e.g. deleted elsewhere).
  useEffect(() => {
    if (selectedId && !suites.some((s) => s.id === selectedId)) {
      setSelectedId(undefined);
      if (!isCreating) setDraft(undefined);
    }
  }, [suites, selectedId, isCreating]);

  const startCreate = useCallback(() => {
    setIsCreating(true);
    setSelectedId(undefined);
    setDraft({ name: "", description: "", capabilities: [] });
    setCapSearch("");
    setKindFilter("all");
    setApplyMsg("");
  }, []);

  const selectSuite = useCallback(
    (id: string) => {
      const suite = suites.find((s) => s.id === id);
      if (!suite) return;
      setIsCreating(false);
      setSelectedId(id);
      setDraft({
        name: suite.name,
        description: suite.description ?? "",
        capabilities: [...suite.capabilities],
      });
      setCapSearch("");
      setKindFilter("all");
      setApplyMsg("");
    },
    [suites],
  );

  const cancelEdit = useCallback(() => {
    if (isCreating) {
      setIsCreating(false);
      setDraft(undefined);
    } else if (selectedId) {
      selectSuite(selectedId);
    } else {
      setDraft(undefined);
    }
  }, [isCreating, selectedId, selectSuite]);

  const save = useCallback(async () => {
    if (!draft || !draft.name.trim()) return;
    setBusy(true);
    try {
      const payload = {
        name: draft.name.trim(),
        description: draft.description.trim() || null,
        capabilities: draft.capabilities,
      };
      if (isCreating) {
        const created = await createSuite(payload);
        setIsCreating(false);
        await reload();
        selectSuite(created.id);
      } else if (selectedId) {
        await updateSuite(selectedId, payload);
        await reload();
      }
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [draft, isCreating, selectedId, reload, selectSuite, props]);

  const remove = useCallback(async () => {
    if (!selectedId) return;
    const suite = suites.find((s) => s.id === selectedId);
    if (!window.confirm(`Delete suite "${suite?.name ?? selectedId}"? This cannot be undone.`))
      return;
    setBusy(true);
    try {
      await deleteSuite(selectedId);
      setSelectedId(undefined);
      setDraft(undefined);
      await reload();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [selectedId, suites, reload, props]);

  const applySelected = useCallback(async () => {
    if (!selectedId) return;
    const suite = suites.find((s) => s.id === selectedId);
    if (
      (!suite || suite.capabilities.length === 0) &&
      !window.confirm(
        "This suite is empty. Applying disables every capability for the tool. Continue?",
      )
    )
      return;
    setBusy(true);
    setApplyMsg("");
    try {
      const result = await applySuite(applyTool, selectedId);
      const ar = result.applyResult;
      setApplyMsg(
        `Applied to ${applyTool} · ${ar.created} added, ${ar.removed} removed` +
          (result.skippedStale > 0 ? `, ${result.skippedStale} stale skipped` : "") +
          (ar.errors.length > 0 ? `, ${ar.errors.length} error(s)` : ""),
      );
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [selectedId, suites, applyTool, props]);

  const included = useMemo(() => new Set(draft?.capabilities ?? []), [draft]);

  const visibleItems = useMemo(() => {
    const q = capSearch.trim().toLowerCase();
    return props.items.filter((it) => {
      if (!SUITE_KINDS.includes(it.kind)) return false;
      if (kindFilter !== "all" && it.kind !== kindFilter) return false;
      if (!q) return true;
      return it.name.toLowerCase().includes(q) || it.relativePath.toLowerCase().includes(q);
    });
  }, [props.items, capSearch, kindFilter]);

  const tree = useMemo(() => buildTree(visibleItems), [visibleItems]);

  const setCapabilities = useCallback((ids: string[], on: boolean) => {
    setDraft((d) => {
      if (!d) return d;
      const set = new Set(d.capabilities);
      for (const id of ids) {
        if (on) set.add(id);
        else set.delete(id);
      }
      return { ...d, capabilities: [...set] };
    });
  }, []);

  const toggleDir = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const effectiveCollapsed = capSearch.trim() ? EMPTY : collapsed;

  return (
    <div className="suites-page">
      <section className="config-panel suite-list-pane">
        <div className="section-header">
          <h2>Suites</h2>
          <button className="btn" onClick={startCreate} disabled={busy}>
            + New
          </button>
        </div>
        {suites.length === 0 && !isCreating ? (
          <div className="suite-empty">No suites yet. Click “+ New” to create one.</div>
        ) : (
          <ul className="suite-list">
            {suites.map((s) => (
              <li
                key={s.id}
                className={`suite-row${s.id === selectedId && !isCreating ? " selected" : ""}`}
                onClick={() => selectSuite(s.id)}
              >
                <span className="suite-name">{s.name}</span>
                <span className="suite-count">{s.capabilities.length} items</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="config-panel suite-editor-pane">
        <div className="section-header">
          <h2>{isCreating ? "New Suite" : draft ? "Edit Suite" : "Editor"}</h2>
          {draft && <span className="src-hint">{draft.capabilities.length} selected</span>}
        </div>
        {selectedId && !isCreating && (
          <div className="suite-apply-row">
            <span className="label">Apply</span>
            <select
              className="suite-tool"
              value={applyTool}
              onChange={(e) => setApplyTool(e.target.value as ToolId)}
              disabled={busy || availableTools.length === 0}
            >
              {availableTools.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
            <button
              className="btn"
              onClick={() => void applySelected()}
              disabled={busy || availableTools.length === 0}
            >
              Apply Suite
            </button>
            {applyMsg && <span className="src-hint">{applyMsg}</span>}
          </div>
        )}
        {!draft ? (
          <div className="suite-empty">Select a suite to edit, or create a new one.</div>
        ) : (
          <div className="suite-editor">
            <div className="field">
              <label className="label">Name</label>
              <input
                className="path-input"
                type="text"
                value={draft.name}
                placeholder="Suite name"
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </div>
            <div className="field">
              <label className="label">Description</label>
              <input
                className="path-input"
                type="text"
                value={draft.description}
                placeholder="Optional description"
                onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              />
            </div>
            <div className="field">
              <label className="label">Capabilities</label>
              <div className="cap-filters">
                <input
                  className="path-input"
                  type="search"
                  placeholder="Search capabilities"
                  value={capSearch}
                  onChange={(e) => setCapSearch(e.target.value)}
                />
                <select
                  className="suite-tool"
                  value={kindFilter}
                  onChange={(e) => setKindFilter(e.target.value as KindFilter)}
                >
                  <option value="all">All kinds</option>
                  <option value="skill">Skills</option>
                  <option value="agent">Agents</option>
                  <option value="rule">Rules</option>
                </select>
              </div>
              <div className="cap-tree">
                {visibleItems.length === 0 ? (
                  <div className="suite-empty">No capabilities match the current filters.</div>
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
            <div className="suite-actions">
              <button className="btn" onClick={() => void save()} disabled={busy || !draft.name.trim()}>
                Save
              </button>
              {!isCreating && (
                <button className="btn-ghost danger" onClick={() => void remove()} disabled={busy}>
                  Delete
                </button>
              )}
              <button className="btn-ghost" onClick={cancelEdit} disabled={busy}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

const EMPTY: ReadonlySet<string> = new Set();

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
    const checked = total > 0 && includedCount === total;
    const indeterminate = includedCount > 0 && includedCount < total;
    const expandable = node.children.length > 0;
    const expanded = expandable && !ctx.collapsed.has(node.id);

    out.push(
      <div className="cap-tree-row" key={node.id} style={{ paddingLeft: depth * 16 + 6 }}>
        {expandable ? (
          <button className="cap-toggle" onClick={() => ctx.onToggleDir(node.id)}>
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className="cap-toggle-placeholder" />
        )}
        <TriCheckbox
          checked={node.type === "item" ? ctx.included.has(node.item!.id) : checked}
          indeterminate={node.type === "item" ? false : indeterminate}
          onChange={(on) => ctx.setCapabilities(node.itemIds, on)}
        />
        <div className="cap-name-wrap">
          <div className="cap-row-name">{node.label}</div>
          <div className="cap-row-path">{node.description}</div>
        </div>
        <span className={`cap-chip${checked || (node.type === "item" && ctx.included.has(node.item!.id)) ? " included" : ""}`}>
          {node.type === "item"
            ? ctx.included.has(node.item!.id)
              ? "Included"
              : node.item!.kind
            : `${includedCount}/${total}`}
        </span>
      </div>,
    );
    if (expanded) out.push(...renderNodes(node.children, depth + 1, ctx));
  }
  return out;
}

function TriCheckbox(props: {
  checked: boolean;
  indeterminate: boolean;
  onChange: (checked: boolean) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = props.indeterminate;
  }, [props.indeterminate]);
  return (
    <input
      ref={ref}
      type="checkbox"
      checked={props.checked}
      onChange={(e) => props.onChange(e.target.checked)}
    />
  );
}
