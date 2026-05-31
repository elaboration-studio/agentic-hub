import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import logoUrl from "./assets/logo.png";
import {
  addSource,
  apply,
  inspect,
  loadSettings,
  onApplyProgress,
  onSourcesChanged,
  plan,
  scaffoldDemo,
  scan,
  setWatcherEnabled,
  syncHooks,
  syncRules,
  type DesiredMap,
} from "./ipc";
import type {
  CapabilityItem,
  InspectResult,
  ScanError,
  Settings,
  ToolCapabilityState,
  ToolId,
} from "./types";
import {
  Banner,
  enabledTools,
  key,
  messageOf,
  WORKSPACE_TOOL_IDS,
  type Route,
  type Scope,
  type ToolDef,
} from "./shared";
import { Matrix } from "./Matrix";
import { WorkspacePanel } from "./WorkspacePanel";
import { ConfigPage } from "./ConfigPage";
import { SuitesPage } from "./SuitesPage";

type Status = "loading" | "ready" | "error";

interface Loaded {
  settings: Settings;
  items: CapabilityItem[];
  scanErrors: ScanError[];
  result: InspectResult;
}

function seedDesired(result: InspectResult): DesiredMap {
  const map: DesiredMap = {};
  for (const s of result.states) {
    map[key(s.tool, s.itemId)] = s.state === "enabled";
  }
  return map;
}

const ROUTES: Route[] = ["manager", "suites", "config"];

function routeFromHash(): Route {
  const hash = window.location.hash.replace(/^#\/?/, "") as Route;
  return ROUTES.includes(hash) ? hash : "manager";
}

export function App() {
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<string>("");
  const [data, setData] = useState<Loaded | null>(null);
  const [desired, setDesired] = useState<DesiredMap>({});
  const [applying, setApplying] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [scope, setScope] = useState<Scope>("global");
  const [route, setRoute] = useState<Route>(routeFromHash);
  const [watching, setWatching] = useState(true);
  const [conflictPrompt, setConflictPrompt] = useState<{
    rows: ConflictRow[];
    resolve: (takeOver: boolean) => void;
  } | null>(null);

  const refresh = useCallback(async () => {
    setStatus("loading");
    setError("");
    try {
      const settings = await loadSettings();
      const { items, errors } = await scan(settings.sources);
      const result = await inspect(items, settings.tools);
      setData({ settings, items, scanErrors: errors, result });
      setDesired(seedDesired(result));
      setWatching(settings.watcherEnabled);
      setStatus("ready");
    } catch (e) {
      setError(messageOf(e));
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const onHash = () => setRoute(routeFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    const unlisten = onApplyProgress((e) =>
      setProgress({ done: e.operationIndex + 1, total: e.totalOperations }),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  // Live-refresh on watcher / resync events. Skip while the user has unapplied
  // edits so an incoming event never discards an in-progress selection.
  const pendingRef = useRef(0);
  useEffect(() => {
    const unlisten = onSourcesChanged(() => {
      if (pendingRef.current === 0) void refresh();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const toggleWatching = useCallback(async (next: boolean) => {
    setWatching(next);
    try {
      await setWatcherEnabled(next);
    } catch (e) {
      setWatching(!next);
      setError(messageOf(e));
    }
  }, []);

  const tools: ToolDef[] = useMemo(() => (data ? enabledTools(data.settings) : []), [data]);
  const workspaceTools = useMemo(
    () => tools.filter((t) => WORKSPACE_TOOL_IDS.has(t.id)),
    [tools],
  );

  const currentMap = useMemo(() => {
    const map = new Map<string, ToolCapabilityState>();
    if (data) {
      for (const s of data.result.states) map.set(key(s.tool, s.itemId), s);
    }
    return map;
  }, [data]);

  const pendingKeys = useMemo(
    () =>
      Object.keys(desired).filter((k) => {
        const cur = currentMap.get(k);
        return cur ? desired[k] !== (cur.state === "enabled") : false;
      }),
    [desired, currentMap],
  );
  pendingRef.current = pendingKeys.length;

  const toggle = useCallback(
    (tool: ToolId, itemId: string) => {
      const k = key(tool, itemId);
      if (!currentMap.has(k)) return;
      setDesired((d) => ({ ...d, [k]: !d[k] }));
    },
    [currentMap],
  );

  const toggleMany = useCallback(
    (tool: ToolId, itemIds: string[], value: boolean) => {
      setDesired((d) => {
        const next = { ...d };
        for (const id of itemIds) {
          const k = key(tool, id);
          if (currentMap.has(k)) next[k] = value;
        }
        return next;
      });
    },
    [currentMap],
  );

  const resetDesired = useCallback(() => {
    if (data) setDesired(seedDesired(data.result));
  }, [data]);

  // Run the plan -> apply -> sync pipeline for every tool with pending edits.
  // `takeOver` authorizes the destructive resolution of `foreign_file` targets.
  const runApply = useCallback(
    async (takeOver: boolean) => {
      if (!data) return;
      setApplying(true);
      setProgress(null);
      setError("");
      try {
        const modifiedTools = new Set<ToolId>();
        for (const k of pendingKeys) modifiedTools.add(toolOfKey(k));
        for (const tool of tools) {
          if (!modifiedTools.has(tool.id)) continue;
          const desiredByItem: DesiredMap = {};
          for (const item of data.items) {
            const k = key(tool.id, item.id);
            if (k in desired) desiredByItem[item.id] = desired[k];
          }
          const ops = await plan(tool.id, data.items, desiredByItem, takeOver);
          if (ops.length > 0) await apply(ops);
          await syncRules(tool.id, data.items, desiredByItem);
          await syncHooks(tool.id, data.items, desiredByItem);
        }
        await refresh();
      } catch (e) {
        setError(messageOf(e));
      } finally {
        setApplying(false);
        setProgress(null);
      }
    },
    [data, desired, pendingKeys, tools, refresh],
  );

  // Surface a confirmation when enabling items whose tool target is blocked by
  // a real file; taking over deletes that file. Other edits apply regardless.
  const applyChanges = useCallback(() => {
    if (!data) return;
    const rows = detectConflicts(pendingKeys, desired, currentMap, data.items, tools);
    if (rows.length === 0) {
      void runApply(false);
      return;
    }
    setConflictPrompt({
      rows,
      resolve: (takeOver) => {
        setConflictPrompt(null);
        void runApply(takeOver);
      },
    });
  }, [data, pendingKeys, desired, currentMap, tools, runApply]);

  return (
    <div className="app">
      <Header
        count={data?.items.length ?? 0}
        sources={data?.settings.sources.length ?? 0}
        watching={watching}
        onToggleWatching={(v) => void toggleWatching(v)}
        route={route}
        scope={scope}
        onScopeChange={setScope}
      />
      <main className="content">
        {status === "error" && <Banner tone="danger">{error}</Banner>}
        {status === "loading" && !data && <Banner tone="muted">Scanning sources…</Banner>}
        {data && route === "config" && (
          <ConfigPage settings={data.settings} onChanged={() => void refresh()} onError={setError} />
        )}
        {data && route === "suites" && (
          <SuitesPage
            items={data.items}
            tools={tools}
            adapterStatuses={data.result.adapterStatuses}
            onError={setError}
          />
        )}
        {data && route === "manager" && (
          <>
            {data.scanErrors.length > 0 && (
              <details className="scan-errors">
                <summary>{data.scanErrors.length} scan notice(s)</summary>
                <ul>
                  {data.scanErrors.map((err, i) => (
                    <li key={i}>
                      <code>{err.path}</code> — {err.message}
                    </li>
                  ))}
                </ul>
              </details>
            )}
            {scope === "global" ? (
              data.items.length === 0 ? (
                <EmptyState
                  destination={
                    data.settings.sources[0]?.path ?? data.settings.sharedRoot
                  }
                  onChanged={() => void refresh()}
                  onError={setError}
                />
              ) : (
                <Matrix
                  items={data.items}
                  tools={tools}
                  currentMap={currentMap}
                  adapterStatuses={data.result.adapterStatuses}
                  desired={desired}
                  onToggle={toggle}
                  onToggleMany={toggleMany}
                  settings={data.settings}
                  onError={setError}
                />
              )
            ) : (
              <WorkspacePanel tools={workspaceTools} onError={setError} />
            )}
          </>
        )}
      </main>
      {route === "manager" && scope === "global" && pendingKeys.length > 0 && (
        <ActionBar
          pending={pendingKeys.length}
          applying={applying}
          progress={progress}
          onApply={() => applyChanges()}
          onReset={resetDesired}
        />
      )}
      {conflictPrompt && (
        <ConflictDialog
          rows={conflictPrompt.rows}
          onTakeOver={() => conflictPrompt.resolve(true)}
          onSkip={() => conflictPrompt.resolve(false)}
        />
      )}
    </div>
  );
}

interface ConflictRow {
  toolLabel: string;
  name: string;
  targetPath: string;
}

// `key()` is `${tool}::${itemId}`; itemId itself never contains "::".
function toolOfKey(k: string): ToolId {
  return k.slice(0, k.indexOf("::")) as ToolId;
}

// Items the user is enabling whose tool target is blocked by a real file.
function detectConflicts(
  pendingKeys: string[],
  desired: DesiredMap,
  currentMap: Map<string, ToolCapabilityState>,
  items: CapabilityItem[],
  tools: ToolDef[],
): ConflictRow[] {
  const rows: ConflictRow[] = [];
  for (const k of pendingKeys) {
    if (!desired[k]) continue;
    const cur = currentMap.get(k);
    if (!cur || cur.state !== "foreign_file") continue;
    const toolId = toolOfKey(k);
    const tool = tools.find((t) => t.id === toolId);
    const item = items.find((it) => it.id === cur.itemId);
    rows.push({
      toolLabel: tool?.label ?? toolId,
      name: item?.name ?? cur.itemId,
      targetPath: cur.targetPath,
    });
  }
  return rows;
}

function ConflictDialog(props: {
  rows: ConflictRow[];
  onTakeOver: () => void;
  onSkip: () => void;
}) {
  return (
    <div className="modal-overlay" role="dialog" aria-modal="true">
      <div className="modal modal-danger">
        <h2>Real files block {props.rows.length} target{props.rows.length === 1 ? "" : "s"}</h2>
        <p className="modal-lead">
          A real file or folder already exists where these capabilities would be
          projected. Taking over <strong>deletes</strong> the existing file/folder
          and replaces it. This cannot be undone.
        </p>
        <ul className="conflict-list">
          {props.rows.map((r, i) => (
            <li key={i}>
              <span className="conflict-where">
                {r.name} <span className="src-hint">in {r.toolLabel}</span>
              </span>
              <code className="conflict-path">{r.targetPath}</code>
            </li>
          ))}
        </ul>
        <div className="modal-actions">
          <button className="btn-ghost" onClick={props.onSkip}>
            Skip these
          </button>
          <button className="btn btn-danger" onClick={props.onTakeOver}>
            Delete &amp; take over
          </button>
        </div>
      </div>
    </div>
  );
}

function navigate(route: Route) {
  // Manager is the default route (empty hash); every other route maps to
  // `#/<route>`. Keep this generic so new routes work without edits here.
  window.location.hash = route === "manager" ? "" : `#/${route}`;
}

function Header(props: {
  count: number;
  sources: number;
  watching: boolean;
  onToggleWatching: (next: boolean) => void;
  route: Route;
  scope: Scope;
  onScopeChange: (s: Scope) => void;
}) {
  return (
    <header className="header">
      <div className="brand">
        <img className="logo" src={logoUrl} alt="" aria-hidden />
        <div>
          <h1>Agentic Hub</h1>
          <p className="subtitle">
            {props.count} capabilities · {props.sources || 1} source
            {props.sources === 1 ? "" : "s"}
          </p>
        </div>
      </div>
      <div className="header-actions">
        <div className="nav" role="tablist">
          <button
            className={`nav-tab${props.route === "manager" ? " active" : ""}`}
            onClick={() => navigate("manager")}
          >
            Manager
          </button>
          <button
            className={`nav-tab${props.route === "suites" ? " active" : ""}`}
            onClick={() => navigate("suites")}
          >
            Suites
          </button>
          <button
            className={`nav-tab${props.route === "config" ? " active" : ""}`}
            onClick={() => navigate("config")}
          >
            Config
          </button>
        </div>
        {props.route === "manager" && (
          <div className="scope-toggle" role="tablist">
            <button
              className={`scope-tab${props.scope === "global" ? " active" : ""}`}
              onClick={() => props.onScopeChange("global")}
            >
              Global
            </button>
            <button
              className={`scope-tab${props.scope === "workspace" ? " active" : ""}`}
              onClick={() => props.onScopeChange("workspace")}
            >
              Workspace
            </button>
          </div>
        )}
        <button
          className={`watch-toggle${props.watching ? " on" : ""}`}
          onClick={() => props.onToggleWatching(!props.watching)}
          title={
            props.watching
              ? "Watching source roots — changes sync automatically. Click to pause."
              : "Watcher paused. Click to watch source roots and auto-sync changes."
          }
          aria-pressed={props.watching}
        >
          <span className="watch-dot" aria-hidden />
          {props.watching ? "Watching" : "Paused"}
        </button>
      </div>
    </header>
  );
}

function EmptyState(props: {
  destination: string;
  onChanged: () => void;
  onError: (msg: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<string>("");

  const onScaffold = useCallback(async () => {
    setBusy(true);
    setSummary("");
    try {
      const r = await scaffoldDemo("merge");
      setSummary(
        `Wrote ${r.written}, skipped ${r.skipped} into ${r.destinationRoot}.`,
      );
      props.onChanged();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [props]);

  const onAddSource = useCallback(async () => {
    setBusy(true);
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Choose a resources root",
      });
      if (typeof picked !== "string") return; // cancelled
      const label = picked.split("/").filter(Boolean).pop() ?? picked;
      await addSource(label, picked);
      props.onChanged();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [props]);

  return (
    <section className="empty-state">
      <h2>No capabilities yet</h2>
      <p className="empty-lead">
        Bootstrap a starter shared root with example skills, agents, rules, and a
        hook — then enable them per tool from the manager.
      </p>
      <code className="empty-dest">{props.destination}</code>
      <div className="empty-actions">
        <button className="btn" onClick={() => void onScaffold()} disabled={busy}>
          {busy ? "Working…" : "Scaffold demo resources"}
        </button>
        <button className="btn-ghost" onClick={() => void onAddSource()} disabled={busy}>
          Add a source…
        </button>
      </div>
      {summary && <p className="src-hint">{summary}</p>}
    </section>
  );
}

function ActionBar(props: {
  pending: number;
  applying: boolean;
  progress: { done: number; total: number } | null;
  onApply: () => void;
  onReset: () => void;
}) {
  return (
    <div className="actionbar">
      <span className="pending">
        {props.pending} pending change{props.pending === 1 ? "" : "s"}
        {props.applying && props.progress
          ? ` · applying ${props.progress.done}/${props.progress.total}`
          : props.applying
            ? " · applying…"
            : ""}
      </span>
      <div className="actionbar-buttons">
        <button className="btn-ghost" onClick={props.onReset} disabled={props.applying}>
          Reset
        </button>
        <button className="btn" onClick={props.onApply} disabled={props.applying}>
          Apply
        </button>
      </div>
    </div>
  );
}
