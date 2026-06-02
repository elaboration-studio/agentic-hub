// Config route: all settings live here (sources, editor, suite-file, tools,
// sync recovery). Reads settings from the manager store and triggers a refresh
// after each change; action errors surface as toasts.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { addSource, removeSource, rescanResync, saveSettings } from "@/ipc";
import type { Settings, ToolId } from "@/types";
import { ALL_TOOLS, messageOf, resolveSourceIds } from "@/shared";
import { useManagerStore } from "@/state/manager";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const sectionTitle = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";
const hint = "text-xs text-muted-foreground";

interface PanelProps {
  settings: Settings;
  onChanged: () => void;
}

export function ConfigPage() {
  const settings = useManagerStore((s) => s.data?.settings);
  const refresh = useManagerStore((s) => s.refresh);
  if (!settings) return null;
  const props: PanelProps = { settings, onChanged: () => void refresh() };
  return (
    <div className="flex flex-col gap-[18px]">
      <SourcesPanel {...props} />
      <EditorPanel {...props} />
      <SuiteFilePanel {...props} />
      <ToolsPanel {...props} />
      <WatcherPanel {...props} />
    </div>
  );
}

function EditorPanel({ settings, onChanged }: PanelProps) {
  const editor = settings.editor;
  const [kind, setKind] = useState(editor.kind);
  const [customApp, setCustomApp] = useState(editor.customApp ?? "");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setKind(settings.editor.kind);
    setCustomApp(settings.editor.customApp ?? "");
  }, [settings.editor]);

  const persist = useCallback(
    async (nextKind: string, nextCustom: string) => {
      setBusy(true);
      try {
        await saveSettings({
          ...settings,
          editor: {
            kind: nextKind,
            customApp: nextKind === "custom" ? nextCustom.trim() || null : null,
          },
        });
        onChanged();
      } catch (e) {
        toast.error(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [settings, onChanged],
  );

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className={sectionTitle}>Editor</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          Which app opens a capability&rsquo;s original file from the row actions menu. Choose
          &ldquo;System default&rdquo; to use the OS file-type association.
        </p>
        <div className="flex flex-wrap items-center gap-2.5">
          <Select
            value={kind}
            disabled={busy}
            onValueChange={(next) => {
              setKind(next);
              if (next !== "custom") void persist(next, customApp);
            }}
          >
            <SelectTrigger className="w-[200px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">System default</SelectItem>
              <SelectItem value="vscode">Visual Studio Code</SelectItem>
              <SelectItem value="cursor">Cursor</SelectItem>
              <SelectItem value="custom">Custom…</SelectItem>
            </SelectContent>
          </Select>
          {kind === "custom" && (
            <>
              <Input
                className="flex-1 font-mono"
                value={customApp}
                placeholder="App name or path (e.g. Zed)"
                onChange={(e) => setCustomApp(e.target.value)}
              />
              <Button onClick={() => void persist("custom", customApp)} disabled={busy}>
                Save
              </Button>
            </>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function WatcherPanel({ onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  const onResync = useCallback(async () => {
    setBusy(true);
    setDone(false);
    try {
      await rescanResync();
      setDone(true);
      onChanged();
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [onChanged]);

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className={sectionTitle}>Sync recovery</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          The source watcher keeps every tool in sync automatically (toggle it from the header). If
          projections ever look out of sync, force a full rescan and resync of all enabled tools and
          the active workspace.
        </p>
        <div className="flex items-center gap-3">
          <Button onClick={() => void onResync()} disabled={busy}>
            {busy ? "Resyncing…" : "Rescan & resync everything"}
          </Button>
          {done && !busy && <span className={hint}>Done — projections reconciled.</span>}
        </div>
      </CardContent>
    </Card>
  );
}

function SourcesPanel({ settings, onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);
  const { sources, sharedRoot } = settings;
  const usingDefault = sources.length === 0;
  const ids = useMemo(() => resolveSourceIds(sources), [sources]);

  const onAdd = useCallback(async () => {
    setBusy(true);
    try {
      const picked = await open({ directory: true, multiple: false, title: "Choose a resources root" });
      if (typeof picked !== "string") return;
      const label = picked.split("/").filter(Boolean).pop() ?? picked;
      await addSource(label, picked);
      onChanged();
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [onChanged]);

  const onRemove = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await removeSource(id);
        onChanged();
      } catch (e) {
        toast.error(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [onChanged],
  );

  return (
    <Card className="p-4">
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Sources</CardTitle>
        <Button variant="ghost" size="sm" onClick={() => void onAdd()} disabled={busy}>
          Add source…
        </Button>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>Ordered shared roots scanned for capabilities. First match wins on collision.</p>
        <ul className="flex flex-wrap gap-2">
          {usingDefault ? (
            <li className="flex items-center gap-2 rounded-lg border bg-secondary px-3 py-1.5">
              <span className="font-semibold">Default</span>
              <code className="font-mono text-[11px] text-muted-foreground">{sharedRoot}</code>
              <span className={hint}>fallback — add your own root to replace it</span>
            </li>
          ) : (
            sources.map((s, i) => (
              <li key={ids[i]} className="flex items-center gap-2 rounded-lg border bg-secondary px-3 py-1.5">
                <span className="font-semibold">{s.label}</span>
                <code className="font-mono text-[11px] text-muted-foreground">{s.path}</code>
                <Button
                  variant="ghost"
                  size="xs"
                  className="text-destructive hover:text-destructive"
                  onClick={() => void onRemove(ids[i])}
                  disabled={busy}
                >
                  Remove
                </Button>
              </li>
            ))
          )}
        </ul>
      </CardContent>
    </Card>
  );
}

function SuiteFilePanel({ settings, onChanged }: PanelProps) {
  const [value, setValue] = useState(settings.suitesPath ?? "");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setValue(settings.suitesPath ?? "");
  }, [settings.suitesPath]);

  const persist = useCallback(
    async (next: string | null) => {
      setBusy(true);
      try {
        await saveSettings({ ...settings, suitesPath: next });
        onChanged();
      } catch (e) {
        toast.error(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [settings, onChanged],
  );

  const onBrowse = useCallback(async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Choose a folder for the suites file" });
      if (typeof dir !== "string") return;
      const next = `${dir}/.agentic-suites.json`;
      setValue(next);
      await persist(next);
    } catch (e) {
      toast.error(messageOf(e));
    }
  }, [persist]);

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className={sectionTitle}>Suite file</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          Where named suites are stored. Default: <code className="font-mono">~/.agentic-suites.json</code>
        </p>
        <div className="flex flex-wrap items-center gap-2.5">
          <Input
            className="flex-1 font-mono"
            value={value}
            placeholder="~/.agentic-suites.json (default)"
            onChange={(e) => setValue(e.target.value)}
          />
          <Button variant="ghost" onClick={() => void onBrowse()} disabled={busy}>
            Browse folder…
          </Button>
          <Button
            onClick={() => void persist(value.trim() === "" ? null : value.trim())}
            disabled={busy}
          >
            Save
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              setValue("");
              void persist(null);
            }}
            disabled={busy || (settings.suitesPath ?? "") === ""}
          >
            Reset to default
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function ToolsPanel({ settings, onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);

  const toggle = useCallback(
    async (id: ToolId, enabled: boolean) => {
      setBusy(true);
      try {
        const next: Settings = {
          ...settings,
          tools: { ...settings.tools, [id]: { ...settings.tools[id], enabled } },
        };
        await saveSettings(next);
        onChanged();
      } catch (e) {
        toast.error(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [settings, onChanged],
  );

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className={sectionTitle}>Tools</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>Enabled tools appear as columns in the manager. More tools coming later.</p>
        <ul className="flex flex-col gap-1.5">
          {ALL_TOOLS.map((t) => {
            const ts = settings.tools[t.id];
            return (
              <li
                key={t.id}
                className="flex items-center justify-between gap-3 rounded-lg border bg-secondary px-2.5 py-2"
              >
                <Label className="flex cursor-pointer items-center gap-2.5">
                  <Switch
                    checked={ts.enabled}
                    disabled={busy}
                    onCheckedChange={(v) => void toggle(t.id, v)}
                  />
                  <span className="font-semibold">{t.label}</span>
                </Label>
                <code className="font-mono text-[11px] text-muted-foreground">{ts.skillsPath}</code>
              </li>
            );
          })}
        </ul>
      </CardContent>
    </Card>
  );
}
