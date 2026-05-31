import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  apply,
  inspect,
  loadSettings,
  onApplyProgress,
  plan,
  scan,
  syncHooks,
  syncRules,
  type DesiredMap,
} from "./ipc";
import type {
  AdapterStatus,
  CapabilityItem,
  CapabilityKind,
  InspectResult,
  LinkState,
  ScanError,
  Settings,
  ToolCapabilityState,
  ToolId,
} from "./types";

const TOOLS: { id: ToolId; label: string }[] = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude" },
  { id: "cursor", label: "Cursor" },
  { id: "openclaw", label: "OpenClaw" },
];

const KIND_ORDER: CapabilityKind[] = ["skill", "agent", "rule", "hook"];
const KIND_LABEL: Record<CapabilityKind, string> = {
  skill: "Skills",
  agent: "Agents",
  rule: "Rules",
  hook: "Hooks",
};

// Current states that are "abnormal" — surfaced as a dot on the toggle.
const ABNORMAL: Record<LinkState, string | null> = {
  enabled: null,
  disabled: null,
  broken: "Broken link",
  stale: "Stale copy",
  foreign_file: "A real file blocks this target",
  foreign_link: "Owned by another source",
};

type Status = "loading" | "ready" | "error";

interface Loaded {
  settings: Settings;
  items: CapabilityItem[];
  scanErrors: ScanError[];
  result: InspectResult;
}

const key = (tool: ToolId, itemId: string) => `${tool}::${itemId}`;

function seedDesired(result: InspectResult): DesiredMap {
  const map: DesiredMap = {};
  for (const s of result.states) {
    map[key(s.tool, s.itemId)] = s.state === "enabled";
  }
  return map;
}

export function App() {
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<string>("");
  const [data, setData] = useState<Loaded | null>(null);
  const [desired, setDesired] = useState<DesiredMap>({});
  const [applying, setApplying] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);

  const refresh = useCallback(async () => {
    setStatus("loading");
    setError("");
    try {
      const settings = await loadSettings();
      const { items, errors } = await scan(settings.sources);
      const result = await inspect(items, settings.tools);
      setData({ settings, items, scanErrors: errors, result });
      setDesired(seedDesired(result));
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
    const unlisten = onApplyProgress((e) =>
      setProgress({ done: e.operationIndex + 1, total: e.totalOperations }),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

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

  const toggle = useCallback(
    (tool: ToolId, itemId: string) => {
      const k = key(tool, itemId);
      if (!currentMap.has(k)) return;
      setDesired((d) => ({ ...d, [k]: !d[k] }));
    },
    [currentMap],
  );

  const resetDesired = useCallback(() => {
    if (data) setDesired(seedDesired(data.result));
  }, [data]);

  const applyChanges = useCallback(async () => {
    if (!data) return;
    setApplying(true);
    setProgress(null);
    setError("");
    try {
      const modifiedTools = new Set<ToolId>();
      for (const k of pendingKeys) {
        modifiedTools.add(k.split("::")[0] as ToolId);
      }
      for (const tool of TOOLS) {
        if (!modifiedTools.has(tool.id)) continue;
        const desiredByItem: DesiredMap = {};
        for (const item of data.items) {
          const k = key(tool.id, item.id);
          if (k in desired) desiredByItem[item.id] = desired[k];
        }
        const ops = await plan(tool.id, data.items, desiredByItem);
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
  }, [data, desired, pendingKeys, refresh]);

  return (
    <div className="app">
      <Header
        count={data?.items.length ?? 0}
        sources={data?.settings.sources.length ?? 0}
        onRefresh={() => void refresh()}
        loading={status === "loading"}
      />
      <main className="content">
        {status === "error" && <Banner tone="danger">{error}</Banner>}
        {status === "loading" && !data && <Banner tone="muted">Scanning sources…</Banner>}
        {data && (
          <>
            <SourceList settings={data.settings} />
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
            <Matrix
              items={data.items}
              currentMap={currentMap}
              adapterStatuses={data.result.adapterStatuses}
              desired={desired}
              onToggle={toggle}
            />
          </>
        )}
      </main>
      {pendingKeys.length > 0 && (
        <ActionBar
          pending={pendingKeys.length}
          applying={applying}
          progress={progress}
          onApply={() => void applyChanges()}
          onReset={resetDesired}
        />
      )}
    </div>
  );
}

function Header(props: {
  count: number;
  sources: number;
  onRefresh: () => void;
  loading: boolean;
}) {
  return (
    <header className="header">
      <div className="brand">
        <span className="logo" aria-hidden />
        <div>
          <h1>Agentic Hub</h1>
          <p className="subtitle">
            {props.count} capabilities · {props.sources || 1} source
            {props.sources === 1 ? "" : "s"}
          </p>
        </div>
      </div>
      <button className="btn" onClick={props.onRefresh} disabled={props.loading}>
        {props.loading ? "Scanning…" : "Rescan"}
      </button>
    </header>
  );
}

function SourceList(props: { settings: Settings }) {
  const sources =
    props.settings.sources.length > 0
      ? props.settings.sources.map((s) => ({ label: s.label, path: s.path }))
      : [{ label: "Default", path: props.settings.sharedRoot }];
  return (
    <section className="sources">
      <h2>Sources</h2>
      <ul>
        {sources.map((s, i) => (
          <li key={i}>
            <span className="src-label">{s.label}</span>
            <code className="src-path">{s.path}</code>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Matrix(props: {
  items: CapabilityItem[];
  currentMap: Map<string, ToolCapabilityState>;
  adapterStatuses: AdapterStatus[];
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
}) {
  const adapterMap = useMemo(() => {
    const map = new Map<ToolId, AdapterStatus>();
    for (const a of props.adapterStatuses) map.set(a.tool, a);
    return map;
  }, [props.adapterStatuses]);

  if (props.items.length === 0) {
    return <Banner tone="muted">No capabilities found in the configured sources.</Banner>;
  }

  return (
    <section className="matrix">
      <table>
        <thead>
          <tr>
            <th className="col-cap">Capability</th>
            <th className="col-src">Source</th>
            {TOOLS.map((t) => {
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
          {KIND_ORDER.map((kind) => {
            const rows = props.items.filter((it) => it.kind === kind);
            if (rows.length === 0) return null;
            return (
              <KindGroup
                key={kind}
                kind={kind}
                rows={rows}
                currentMap={props.currentMap}
                adapterMap={adapterMap}
                desired={props.desired}
                onToggle={props.onToggle}
              />
            );
          })}
        </tbody>
      </table>
    </section>
  );
}

function KindGroup(props: {
  kind: CapabilityKind;
  rows: CapabilityItem[];
  currentMap: Map<string, ToolCapabilityState>;
  adapterMap: Map<ToolId, AdapterStatus>;
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
}) {
  return (
    <>
      <tr className="kind-row">
        <td colSpan={2 + TOOLS.length}>
          {KIND_LABEL[props.kind]} <span className="kind-count">{props.rows.length}</span>
        </td>
      </tr>
      {props.rows.map((item) => (
        <tr key={item.id} className={item.valid ? "" : "invalid"}>
          <td className="col-cap">
            <span className="cap-name">{item.name}</span>
            <code className="cap-rel">{item.relativePath}</code>
          </td>
          <td className="col-src">{item.sourceLabel}</td>
          {TOOLS.map((t) => {
            const adapter = props.adapterMap.get(t.id);
            const k = key(t.id, item.id);
            const cur = props.currentMap.get(k);
            if ((adapter && !adapter.available) || !cur) {
              return (
                <td key={t.id} className="cell">
                  <span className="dash">—</span>
                </td>
              );
            }
            const on = props.desired[k] ?? false;
            const modified = on !== (cur.state === "enabled");
            const abnormal = ABNORMAL[cur.state];
            return (
              <td key={t.id} className="cell">
                <button
                  className={`toggle${on ? " on" : ""}${modified ? " mod" : ""}${
                    abnormal ? " warn" : ""
                  }`}
                  title={`current: ${cur.state}${abnormal ? ` — ${abnormal}` : ""}`}
                  onClick={() => props.onToggle(t.id, item.id)}
                >
                  {on ? "✓" : ""}
                  {abnormal && <span className="warn-dot" />}
                </button>
              </td>
            );
          })}
        </tr>
      ))}
    </>
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

function Banner(props: { tone: "danger" | "muted"; children: ReactNode }) {
  return <div className={`banner banner-${props.tone}`}>{props.children}</div>;
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
