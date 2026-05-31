// Config route: all settings live here (sources, suite-file location, tools).
// Future configuration surfaces should be added as panels on this page.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { addSource, removeSource, saveSettings } from "./ipc";
import type { Settings, ToolId } from "./types";
import { ALL_TOOLS, messageOf, resolveSourceIds } from "./shared";

interface PanelProps {
  settings: Settings;
  onChanged: () => void;
  onError: (msg: string) => void;
}

export function ConfigPage(props: PanelProps) {
  return (
    <div className="config-page">
      <SourcesPanel {...props} />
      <SuiteFilePanel {...props} />
      <ToolsPanel {...props} />
    </div>
  );
}

function SourcesPanel(props: PanelProps) {
  const [busy, setBusy] = useState(false);
  const { sources, sharedRoot } = props.settings;
  const usingDefault = sources.length === 0;
  const ids = useMemo(() => resolveSourceIds(sources), [sources]);

  const onAdd = useCallback(async () => {
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

  const onRemove = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await removeSource(id);
        props.onChanged();
      } catch (e) {
        props.onError(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [props],
  );

  return (
    <section className="config-panel sources">
      <div className="sources-head">
        <h2>Sources</h2>
        <button className="btn-ghost" onClick={() => void onAdd()} disabled={busy}>
          Add source…
        </button>
      </div>
      <p className="src-hint">Ordered shared roots scanned for capabilities. First match wins on collision.</p>
      <ul>
        {usingDefault ? (
          <li>
            <span className="src-label">Default</span>
            <code className="src-path">{sharedRoot}</code>
            <span className="src-hint">fallback — add your own root to replace it</span>
          </li>
        ) : (
          sources.map((s, i) => (
            <li key={ids[i]}>
              <span className="src-label">{s.label}</span>
              <code className="src-path">{s.path}</code>
              <button
                className="btn-ghost danger src-remove"
                onClick={() => void onRemove(ids[i])}
                disabled={busy}
              >
                Remove
              </button>
            </li>
          ))
        )}
      </ul>
    </section>
  );
}

function SuiteFilePanel(props: PanelProps) {
  const [value, setValue] = useState(props.settings.suitesPath ?? "");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setValue(props.settings.suitesPath ?? "");
  }, [props.settings.suitesPath]);

  const persist = useCallback(
    async (next: string | null) => {
      setBusy(true);
      try {
        await saveSettings({ ...props.settings, suitesPath: next });
        props.onChanged();
      } catch (e) {
        props.onError(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [props],
  );

  const onBrowse = useCallback(async () => {
    try {
      const dir = await open({
        directory: true,
        multiple: false,
        title: "Choose a folder for the suites file",
      });
      if (typeof dir !== "string") return;
      const next = `${dir}/.agentic-suites.json`;
      setValue(next);
      await persist(next);
    } catch (e) {
      props.onError(messageOf(e));
    }
  }, [persist, props]);

  return (
    <section className="config-panel">
      <h2>Suite file</h2>
      <p className="src-hint">
        Where named suites are stored. Default: <code>~/.agentic-suites.json</code>
      </p>
      <div className="suitefile-row">
        <input
          className="path-input"
          value={value}
          placeholder="~/.agentic-suites.json (default)"
          onChange={(e) => setValue(e.target.value)}
        />
        <button className="btn-ghost" onClick={() => void onBrowse()} disabled={busy}>
          Browse folder…
        </button>
        <button
          className="btn"
          onClick={() => void persist(value.trim() === "" ? null : value.trim())}
          disabled={busy}
        >
          Save
        </button>
        <button
          className="btn-ghost"
          onClick={() => {
            setValue("");
            void persist(null);
          }}
          disabled={busy || (props.settings.suitesPath ?? "") === ""}
        >
          Reset to default
        </button>
      </div>
    </section>
  );
}

function ToolsPanel(props: PanelProps) {
  const [busy, setBusy] = useState(false);

  const toggle = useCallback(
    async (id: ToolId, enabled: boolean) => {
      setBusy(true);
      try {
        const next: Settings = {
          ...props.settings,
          tools: {
            ...props.settings.tools,
            [id]: { ...props.settings.tools[id], enabled },
          },
        };
        await saveSettings(next);
        props.onChanged();
      } catch (e) {
        props.onError(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [props],
  );

  return (
    <section className="config-panel">
      <h2>Tools</h2>
      <p className="src-hint">Enabled tools appear as columns in the manager. More tools coming later.</p>
      <ul className="tools-list">
        {ALL_TOOLS.map((t) => {
          const ts = props.settings.tools[t.id];
          return (
            <li key={t.id} className="tool-row">
              <label className="tool-toggle">
                <input
                  type="checkbox"
                  checked={ts.enabled}
                  disabled={busy}
                  onChange={(e) => void toggle(t.id, e.target.checked)}
                />
                <span className="tool-name">{t.label}</span>
              </label>
              <code className="src-path">{ts.skillsPath}</code>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
