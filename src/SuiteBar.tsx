// Suite picker / apply / save-as bar for the global manager view.

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  applySuite,
  createSuite,
  deleteSuite,
  listSuites,
  onSuiteStoreChanged,
} from "./ipc";
import type { AdapterStatus, SuiteDefinition, ToolId } from "./types";
import { messageOf, type ToolDef } from "./shared";

export function SuiteBar(props: {
  tools: ToolDef[];
  adapterStatuses: AdapterStatus[];
  enabledCapsFor: (tool: ToolId) => string[];
  onApplied: () => void;
  onError: (msg: string) => void;
}) {
  const [suites, setSuites] = useState<SuiteDefinition[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [tool, setTool] = useState<ToolId>(props.tools[0]?.id ?? "codex");
  const [busy, setBusy] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  const availableTools = useMemo(
    () => props.tools.filter((t) => props.adapterStatuses.find((a) => a.tool === t.id)?.available),
    [props.tools, props.adapterStatuses],
  );

  useEffect(() => {
    // Keep the selected tool valid as enablement changes.
    setTool((cur) =>
      availableTools.some((t) => t.id === cur) ? cur : (availableTools[0]?.id ?? cur),
    );
  }, [availableTools]);

  const reload = useCallback(async () => {
    try {
      const list = await listSuites();
      setSuites(list);
      setSelectedId((cur) => (list.some((s) => s.id === cur) ? cur : (list[0]?.id ?? "")));
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

  const onApply = useCallback(async () => {
    if (!selectedId) return;
    const suite = suites.find((s) => s.id === selectedId);
    const empty = !suite || suite.capabilities.length === 0;
    if (
      empty &&
      !window.confirm(
        "This suite is empty. Applying disables every capability for the tool. Continue?",
      )
    )
      return;
    setBusy(true);
    try {
      const result = await applySuite(tool, selectedId);
      if (result.skippedStale > 0) {
        props.onError(`Applied with ${result.skippedStale} stale reference(s) skipped.`);
      }
      props.onApplied();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [selectedId, suites, tool, props]);

  const onCreate = useCallback(async () => {
    const name = newName.trim();
    if (!name) return;
    setBusy(true);
    try {
      await createSuite({ name, capabilities: props.enabledCapsFor(tool) });
      setNewName("");
      setCreating(false);
      await reload();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [newName, props, tool, reload]);

  const onDelete = useCallback(async () => {
    if (!selectedId) return;
    const suite = suites.find((s) => s.id === selectedId);
    if (!window.confirm(`Delete suite "${suite?.name ?? selectedId}"?`)) return;
    setBusy(true);
    try {
      await deleteSuite(selectedId);
      await reload();
    } catch (e) {
      props.onError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [selectedId, suites, reload, props]);

  const selected = suites.find((s) => s.id === selectedId);

  return (
    <section className="suitebar">
      <h2>Suites</h2>
      <div className="suitebar-row">
        <select
          className="suite-select"
          value={selectedId}
          onChange={(e) => setSelectedId(e.target.value)}
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
          {availableTools.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
        <button className="btn" onClick={() => void onApply()} disabled={busy || !selectedId}>
          Apply Suite
        </button>
        <button className="btn-ghost" onClick={() => setCreating((c) => !c)} disabled={busy}>
          {creating ? "Cancel" : "Save current as…"}
        </button>
        <button
          className="btn-ghost danger"
          onClick={() => void onDelete()}
          disabled={busy || !selectedId}
        >
          Delete
        </button>
      </div>
      {creating && (
        <div className="suitebar-row suite-create">
          <input
            className="suite-name-input"
            placeholder={`Suite name (captures ${props.enabledCapsFor(tool).length} enabled from ${tool})`}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void onCreate();
            }}
          />
          <button className="btn" onClick={() => void onCreate()} disabled={busy || !newName.trim()}>
            Save
          </button>
        </div>
      )}
      {selected && selected.description && <p className="suite-desc">{selected.description}</p>}
    </section>
  );
}
