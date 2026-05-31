import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { inspect, loadSettings, scan } from "./ipc";
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

const STATE_META: Record<LinkState, { label: string; tone: string }> = {
  enabled: { label: "Enabled", tone: "ok" },
  disabled: { label: "Off", tone: "muted" },
  broken: { label: "Broken", tone: "danger" },
  stale: { label: "Stale", tone: "warn" },
  foreign_file: { label: "Foreign file", tone: "alien" },
  foreign_link: { label: "Foreign link", tone: "alien" },
};

type Status = "loading" | "ready" | "error";

interface Loaded {
  settings: Settings;
  items: CapabilityItem[];
  scanErrors: ScanError[];
  result: InspectResult;
}

const stateKey = (tool: ToolId, itemId: string) => `${tool}::${itemId}`;

export function App() {
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<string>("");
  const [data, setData] = useState<Loaded | null>(null);

  const refresh = useCallback(async () => {
    setStatus("loading");
    setError("");
    try {
      const settings = await loadSettings();
      const { items, errors } = await scan(settings.sources);
      const result = await inspect(items, settings.tools);
      setData({ settings, items, scanErrors: errors, result });
      setStatus("ready");
    } catch (e) {
      const message =
        e && typeof e === "object" && "message" in e
          ? String((e as { message: unknown }).message)
          : String(e);
      setError(message);
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="app">
      <Header
        count={data?.items.length ?? 0}
        sources={data?.settings.sources.length ?? 0}
        onRefresh={() => void refresh()}
        loading={status === "loading"}
      />
      <main className="content">
        {status === "error" && <Banner tone="danger">Failed to load: {error}</Banner>}
        {status === "loading" && !data && <Banner tone="muted">Scanning sources…</Banner>}
        {data && (
          <>
            <SourceList settings={data.settings} items={data.items} />
            {data.scanErrors.length > 0 && (
              <details className="scan-errors" open>
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
            <Matrix items={data.items} result={data.result} />
          </>
        )}
      </main>
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

function SourceList(props: { settings: Settings; items: CapabilityItem[] }) {
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

function Matrix(props: { items: CapabilityItem[]; result: InspectResult }) {
  const stateMap = useMemo(() => {
    const map = new Map<string, ToolCapabilityState>();
    for (const s of props.result.states) {
      map.set(stateKey(s.tool, s.itemId), s);
    }
    return map;
  }, [props.result.states]);

  const adapterMap = useMemo(() => {
    const map = new Map<ToolId, AdapterStatus>();
    for (const a of props.result.adapterStatuses) {
      map.set(a.tool, a);
    }
    return map;
  }, [props.result.adapterStatuses]);

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
                stateMap={stateMap}
                adapterMap={adapterMap}
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
  stateMap: Map<string, ToolCapabilityState>;
  adapterMap: Map<ToolId, AdapterStatus>;
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
            if (adapter && !adapter.available) {
              return (
                <td key={t.id} className="cell">
                  <span className="dash">—</span>
                </td>
              );
            }
            const st = props.stateMap.get(stateKey(t.id, item.id));
            return (
              <td key={t.id} className="cell">
                {st ? <StateBadge state={st.state} /> : <span className="dash">·</span>}
              </td>
            );
          })}
        </tr>
      ))}
    </>
  );
}

function StateBadge(props: { state: LinkState }) {
  const meta = STATE_META[props.state];
  return <span className={`badge badge-${meta.tone}`}>{meta.label}</span>;
}

function Banner(props: { tone: "danger" | "muted"; children: ReactNode }) {
  return <div className={`banner banner-${props.tone}`}>{props.children}</div>;
}
