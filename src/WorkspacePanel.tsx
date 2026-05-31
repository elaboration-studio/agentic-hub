// Workspace-scope panel: pick project folders and hard-copy a suite into one.

import { useCallback, useEffect, useState } from "react";
import {
  applyWorkspacePatch,
  listSuites,
  listWorkspaceTargets,
  pickWorkspaceDir,
  removeWorkspaceTarget,
  setActiveWorkspaceTarget,
} from "./ipc";
import type {
  SuiteDefinition,
  ToolId,
  WorkspacePatchResult,
  WorkspaceTarget,
} from "./types";
import { Banner, messageOf, type ToolDef } from "./shared";

export function WorkspacePanel(props: { tools: ToolDef[]; onError: (msg: string) => void }) {
  const [targets, setTargets] = useState<WorkspaceTarget[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [suites, setSuites] = useState<SuiteDefinition[]>([]);
  const [suiteId, setSuiteId] = useState<string>("");
  const [tool, setTool] = useState<ToolId>(props.tools[0]?.id ?? "codex");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<WorkspacePatchResult | null>(null);

  useEffect(() => {
    setTool((cur) => (props.tools.some((t) => t.id === cur) ? cur : (props.tools[0]?.id ?? cur)));
  }, [props.tools]);

  const reload = useCallback(async () => {
    try {
      const [state, suiteList] = await Promise.all([listWorkspaceTargets(), listSuites()]);
      setTargets(state.workspaceTargets);
      setActiveId(state.workspaceActiveId ?? state.workspaceTargets[0]?.id ?? "");
      setSuites(suiteList);
      setSuiteId((cur) => (suiteList.some((s) => s.id === cur) ? cur : (suiteList[0]?.id ?? "")));
    } catch (e) {
      props.onError(messageOf(e));
    }
  }, [props]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const onPick = useCallback(async () => {
    setBusy(true);
    try {
      const target = await pickWorkspaceDir();
      await reload();
      setActiveId(target.id);
    } catch (e) {
      const msg = messageOf(e);
      if (!msg.includes("No folder selected")) props.onError(msg);
    } finally {
      setBusy(false);
    }
  }, [reload, props]);

  const onActivate = useCallback(
    async (id: string) => {
      setActiveId(id);
      try {
        await setActiveWorkspaceTarget(id);
      } catch (e) {
        props.onError(messageOf(e));
      }
    },
    [props],
  );

  const onRemove = useCallback(
    async (id: string) => {
      try {
        await removeWorkspaceTarget(id);
        await reload();
      } catch (e) {
        props.onError(messageOf(e));
      }
    },
    [reload, props],
  );

  const onApply = useCallback(async () => {
    if (!activeId || !suiteId) return;
    setBusy(true);
    setResult(null);
    try {
      const res = await applyWorkspacePatch(activeId, tool, suiteId);
      setResult(res);
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [activeId, suiteId, tool, props]);

  return (
    <section className="workspace">
      <div className="workspace-head">
        <h2>Workspace Targets</h2>
        <button className="btn-ghost" onClick={() => void onPick()} disabled={busy}>
          Add workspace…
        </button>
      </div>
      {targets.length === 0 ? (
        <Banner tone="muted">
          No workspace folders yet. Add one to project a suite into it.
        </Banner>
      ) : (
        <ul className="ws-list">
          {targets.map((t) => (
            <li key={t.id} className={t.id === activeId ? "ws-item active" : "ws-item"}>
              <label className="ws-pick">
                <input
                  type="radio"
                  name="ws-active"
                  checked={t.id === activeId}
                  onChange={() => void onActivate(t.id)}
                />
                <span className="ws-label">{t.label}</span>
                <code className="ws-dir">{t.dir}</code>
              </label>
              <button
                className="btn-ghost danger"
                onClick={() => void onRemove(t.id)}
                disabled={busy}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="workspace-apply">
        <select
          className="suite-select"
          value={suiteId}
          onChange={(e) => setSuiteId(e.target.value)}
          disabled={busy || suites.length === 0}
        >
          {suites.length === 0 && <option value="">No suites yet</option>}
          {suites.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name} ({s.capabilities.length})
            </option>
          ))}
        </select>
        <span className="suite-arrow">→</span>
        <select
          className="suite-tool"
          value={tool}
          onChange={(e) => setTool(e.target.value as ToolId)}
          disabled={busy}
        >
          {props.tools.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
        <button
          className="btn"
          onClick={() => void onApply()}
          disabled={busy || !activeId || !suiteId}
        >
          {busy ? "Applying…" : "Apply Patch"}
        </button>
      </div>

      {result && (
        <div className="ws-result">
          <p>
            Applied <strong>{result.suiteName}</strong> to {result.tool} · {result.applied.length}{" "}
            written, {result.removed.length} cleaned
            {result.skippedStaleIds.length > 0
              ? `, ${result.skippedStaleIds.length} stale skipped`
              : ""}
          </p>
          {result.notes.map((n, i) => (
            <p key={`n${i}`} className="ws-note">
              {n}
            </p>
          ))}
          {result.errors.map((er, i) => (
            <p key={`e${i}`} className="ws-err">
              {er}
            </p>
          ))}
        </div>
      )}
    </section>
  );
}
