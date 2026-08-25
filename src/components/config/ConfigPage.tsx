// Config route: all settings live here (sources, editor, suite-file, tools,
// sync recovery). Reads settings from the manager store and triggers a refresh
// after each change; action errors surface as toasts.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { toast } from "sonner";
import {
  addSource,
  openUrl,
  removeSource,
  rescanResync,
  restartApp,
  revealPath,
  saveSettings,
  setUsageTracingEnabled,
  skillCliCheck,
  syncUsageTracerHooks,
  usageTracingStatus,
} from "@/ipc";
import type { Settings, SkillCliStatus, ToolId, UsageTracingStatus } from "@/types";
import { ALL_TOOLS, messageOf, resolveSourceIds, TOOL_LABELS } from "@/shared";
import { getToolProjectionTargets } from "@/lib/toolTargets";
import { formatLocalTimestamp } from "@/lib/format";
import { useManagerStore } from "@/state/manager";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
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
import { AppearancePanel } from "@/components/config/AppearancePanel";
import { ShortcutPanel } from "@/components/config/ShortcutPanel";
import { Alert, AlertDescription } from "@/components/ui/alert";

const sectionTitle = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";
const hint = "text-xs text-muted-foreground";

// Anchor navigation is a no-op inside the WebView; open external links via Rust.
function openExternal(url: string): void {
  void openUrl(url).catch((e) => toast.error(messageOf(e)));
}

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
      <AppearancePanel colorScheme={settings.colorScheme} onChanged={props.onChanged} />
      <SourcesPanel {...props} />
      <EditorPanel {...props} />
      <ShortcutPanel {...props} />
      <PastePanel {...props} />
      <SuiteFilePanel {...props} />
      <SkillsSourcePanel {...props} />
      <UsageTracingPanel {...props} />
      <ToolsPanel {...props} />
      <WatcherPanel {...props} />
      <TelemetryPanel {...props} />
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

function PastePanel({ settings, onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);

  const persist = useCallback(
    async (next: boolean) => {
      setBusy(true);
      try {
        await saveSettings({ ...settings, pasteIntoFocused: next });
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
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Paste into focused app</CardTitle>
        <Switch
          checked={settings.pasteIntoFocused}
          disabled={busy}
          onCheckedChange={(v) => void persist(v)}
          aria-label="Paste command bodies directly into the focused app"
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          macOS only. When on, choosing a command in the palette copies its body and also pastes
          it into the app you were using (Alfred-style). Requires Accessibility permission for
          Agentic Hub in System Settings → Privacy &amp; Security → Accessibility.
        </p>
      </CardContent>
    </Card>
  );
}

function WatcherPanel({ onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const watching = useManagerStore((s) => s.watching);
  const toggleWatching = useManagerStore((s) => s.toggleWatching);

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
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Source watcher</CardTitle>
        <Switch
          checked={watching}
          onCheckedChange={(v) => void toggleWatching(v)}
          aria-label={watching ? "Watching — pause the watcher" : "Paused — enable the watcher"}
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          When on, the watcher keeps every tool in sync automatically as your source roots change —
          leave it on. If projections ever look out of sync, force a full rescan and resync of all
          enabled tools and the active workspace.
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

function SkillsSourcePanel({ settings, onChanged }: PanelProps) {
  const skills = settings.skills;
  const [favPath, setFavPath] = useState(skills.favoritesPath ?? "");
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [cli, setCli] = useState<SkillCliStatus | null>(null);

  useEffect(() => {
    setFavPath(settings.skills.favoritesPath ?? "");
  }, [settings.skills]);

  const persist = useCallback(
    async (next: Settings["skills"]) => {
      setBusy(true);
      try {
        await saveSettings({ ...settings, skills: next });
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
      const dir = await open({
        directory: true,
        multiple: false,
        title: "Choose a folder for the starred-skills file",
      });
      if (typeof dir !== "string") return;
      const next = `${dir}/skills-favorites.json`;
      setFavPath(next);
      await persist({ ...skills, favoritesPath: next });
    } catch (e) {
      toast.error(messageOf(e));
    }
  }, [persist, skills]);

  const onCheck = useCallback(async () => {
    setChecking(true);
    setCli(null);
    try {
      setCli(await skillCliCheck("skills.sh"));
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setChecking(false);
    }
  }, []);

  return (
    <Card className="p-4">
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Skills.sh source</CardTitle>
        <Switch
          checked={skills.enabled}
          disabled={busy}
          onCheckedChange={(v) => void persist({ ...skills, enabled: v })}
          aria-label="Enable skills.sh source"
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-3 p-0">
        <p className={hint}>
          Search public skills on{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void openExternal("https://skills.sh")}
          >
            skills.sh
          </button>
          , star favorites, and install them into a workspace from the Workspace
          scope. Search uses the public skills.sh index — no key needed. Node
          (npx) is required for installs.
        </p>
        {skills.enabled && (
          <>
            <div className="flex flex-col gap-1.5">
              <Label className="text-xs font-medium">Starred-skills file</Label>
              <p className={hint}>
                Where favorites are stored. Default:{" "}
                <code className="font-mono">~/.agentic-hub/skills-favorites.json</code>
              </p>
              <div className="flex flex-wrap items-center gap-2.5">
                <Input
                  className="flex-1 font-mono"
                  value={favPath}
                  placeholder="~/.agentic-hub/skills-favorites.json (default)"
                  onChange={(e) => setFavPath(e.target.value)}
                />
                <Button variant="ghost" onClick={() => void onBrowse()} disabled={busy}>
                  Browse folder…
                </Button>
                <Button
                  onClick={() =>
                    void persist({
                      ...skills,
                      favoritesPath: favPath.trim() === "" ? null : favPath.trim(),
                    })
                  }
                  disabled={busy}
                >
                  Save
                </Button>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <Button variant="ghost" onClick={() => void onCheck()} disabled={checking}>
                {checking ? "Checking…" : "Check CLI"}
              </Button>
              {cli && (
                <span className={hint}>
                  CLI:{" "}
                  <span className={cli.available ? "text-success" : "text-destructive"}>
                    {cli.available ? "ready" : "missing"}
                  </span>{" "}
                  — {cli.message}
                </span>
              )}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function UsageTracingPanel({ settings, onChanged }: PanelProps) {
  const [busy, setBusy] = useState(false);
  const [syncBusy, setSyncBusy] = useState(false);
  const [syncDone, setSyncDone] = useState(false);
  const [status, setStatus] = useState<UsageTracingStatus | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await usageTracingStatus());
    } catch (e) {
      toast.error(messageOf(e));
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus, settings.usageTracing.enabled]);

  const persist = useCallback(
    async (enabled: boolean) => {
      setBusy(true);
      try {
        setStatus(await setUsageTracingEnabled(enabled));
        onChanged();
      } catch (e) {
        toast.error(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [onChanged],
  );

  const onReloadHooks = useCallback(async () => {
    setSyncBusy(true);
    setSyncDone(false);
    try {
      const result = await syncUsageTracerHooks();
      setStatus(result.status);
      setSyncDone(true);
      if (result.syncedTools.length > 0) {
        const labels = result.syncedTools.map((tool) => TOOL_LABELS[tool]).join(", ");
        toast.success(`Tracer hooks synced to ${labels}. Restart each tool to pick up changes.`);
      } else {
        toast.message("Tracer hooks updated. Enable tracing and at least one capture tool to install hooks.");
      }
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setSyncBusy(false);
    }
  }, []);

  return (
    <Card className="p-4">
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Local usage tracing</CardTitle>
        <Switch
          checked={settings.usageTracing.enabled}
          disabled={busy}
          onCheckedChange={(v) => void persist(v)}
          aria-label="Enable local skill usage tracing"
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          Enabled by default. Agentic Hub installs managed tracer hooks for
          supported enabled tools and stores normalized skill usage events in a
          local SQLite database. Nothing is synced remotely.
        </p>
        <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
          <span>
            Collector:{" "}
            <span className={status?.collectorRunning ? "text-success" : "text-warning"}>
              {status?.collectorRunning ? "running" : "stopped"}
            </span>
          </span>
          <span>Port: {status?.collectorPort ?? settings.usageTracing.collectorPort}</span>
          {status && (
            <span>
              Events: {status.storedEventCount} stored · {status.resolvedEventCount} resolved ·{" "}
              {status.unresolvedEventCount} unresolved
            </span>
          )}
          {status?.dbPath && (
            <button
              type="button"
              className="truncate underline"
              onClick={() => void revealPath(status.dbPath).catch((e) => toast.error(messageOf(e)))}
            >
              Reveal local DB
            </button>
          )}
        </div>
        {status && (
          <div className="grid gap-2 sm:grid-cols-3">
            {status.toolDiagnostics.map((diagnostic) => (
              <div
                key={diagnostic.tool}
                className="rounded-md border bg-secondary/30 px-3 py-2 text-xs"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium">{TOOL_LABELS[diagnostic.tool]}</span>
                  <span className={diagnostic.hookInstalled ? "text-success" : "text-warning"}>
                    {diagnostic.hookInstalled ? "hook installed" : "hook missing"}
                  </span>
                </div>
                <p className="mt-1 text-muted-foreground">
                  {diagnostic.resolvedEventCount} resolved · {diagnostic.unresolvedEventCount}{" "}
                  unresolved
                </p>
                <p className="truncate text-muted-foreground">
                  Last captured: {diagnostic.lastCapturedAt
                    ? formatLocalTimestamp(diagnostic.lastCapturedAt)
                    : "never"}
                </p>
              </div>
            ))}
          </div>
        )}
        {settings.usageTracing.enabled && status && !status.collectorRunning && (
          <Alert variant="destructive">
            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
              <span>Local usage tracing is paused. Restart Agentic Hub to resume collection.</span>
              <Button variant="outline" size="sm" onClick={() => void restartApp()}>
                Restart app
              </Button>
            </AlertDescription>
          </Alert>
        )}
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="ghost"
            onClick={() => void onReloadHooks()}
            disabled={syncBusy || !settings.usageTracing.enabled}
          >
            {syncBusy ? "Syncing hooks…" : "Reload tracer hooks"}
          </Button>
          {syncDone && !syncBusy && settings.usageTracing.enabled && (
            <span className={hint}>
              Hooks synced — restart Codex, Claude, Cursor, Kiro, or Grok to apply.
            </span>
          )}
        </div>
        {!settings.usageTracing.enabled && (
          <p className={hint}>Enable tracing to push managed tracer hooks to your agentic tools.</p>
        )}
      </CardContent>
    </Card>
  );
}

function TelemetryPanel({ settings, onChanged }: PanelProps) {
  const telemetry = settings.telemetry;
  const [busy, setBusy] = useState(false);

  const persist = useCallback(
    async (next: Settings["telemetry"]) => {
      setBusy(true);
      try {
        await saveSettings({ ...settings, telemetry: next });
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
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Usage telemetry</CardTitle>
        <Switch
          checked={telemetry.enabled}
          disabled={busy}
          onCheckedChange={(v) => void persist({ enabled: v })}
          aria-label="Enable anonymous usage telemetry"
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          On by default. When enabled, the app sends anonymous lifecycle events
          (app start and exit), at most one daily-active ping when you actually
          use the app, plus your OS and app version, to{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void openExternal("https://aptabase.com")}
          >
            Aptabase
          </button>
          {" "}to help improve the tool. No file contents, paths, or personal data
          are ever sent. See the{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void openExternal("https://aptabase.com/legal/privacy")}
          >
            privacy policy
          </button>
          .
        </p>
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

  const revealTarget = useCallback(async (path: string, label: string) => {
    try {
      await revealPath(path);
    } catch (e) {
      toast.error(messageOf(e), { description: `${label}: ${path}` });
    }
  }, []);

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className={sectionTitle}>Tools</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className={hint}>
          Enabled tools appear as columns in the manager. Kiro, Copilot, Antigravity, and Grok are
          off by default — enable them here when you use those tools. Expand a tool to see where the hub
          writes skills, agents, rules, hooks, and commands; use{" "}
          <FolderOpen className="inline h-3 w-3 align-text-bottom" aria-hidden /> to reveal the
          target in Finder.
        </p>
        <Accordion type="single" collapsible className="flex flex-col gap-2">
          {ALL_TOOLS.map((t) => {
            const ts = settings.tools[t.id];
            const targets = getToolProjectionTargets(t.id, ts);
            return (
              <AccordionItem
                key={t.id}
                value={t.id}
                className="overflow-hidden rounded-lg border bg-secondary last:border-b"
              >
                <div className="flex items-center gap-2 px-2.5">
                  <Label className="flex shrink-0 cursor-pointer items-center py-2">
                    <Switch
                      checked={ts.enabled}
                      disabled={busy}
                      onCheckedChange={(v) => void toggle(t.id, v)}
                    />
                  </Label>
                  <AccordionTrigger className="py-2 hover:no-underline">
                    <span className="font-semibold">{t.label}</span>
                  </AccordionTrigger>
                </div>
                <AccordionContent className="pb-0">
                  <ul className="border-t border-border/40 px-2.5 py-1">
                    {targets.map((target) => (
                      <li
                        key={target.kind}
                        className="grid grid-cols-[4.5rem_minmax(0,1fr)_auto] items-center gap-x-2 py-1"
                      >
                        <span className="text-[11px] text-muted-foreground">{target.label}</span>
                        {target.unsupported ? (
                          <span className="col-span-2 text-[11px] italic text-muted-foreground">
                            Not projected
                          </span>
                        ) : (
                          <>
                            <div className="min-w-0">
                              <code
                                className="block truncate font-mono text-[11px] text-muted-foreground"
                                title={target.path ?? undefined}
                              >
                                {target.path}
                              </code>
                              {target.detail && (
                                <span className="block truncate text-[10px] text-muted-foreground/80">
                                  {target.detail}
                                </span>
                              )}
                            </div>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              className="h-7 w-7 shrink-0"
                              disabled={busy || !target.path}
                              aria-label={`Reveal ${t.label} ${target.label} target`}
                              onClick={() => void revealTarget(target.path!, target.label)}
                            >
                              <FolderOpen className="h-3.5 w-3.5" />
                            </Button>
                          </>
                        )}
                      </li>
                    ))}
                  </ul>
                </AccordionContent>
              </AccordionItem>
            );
          })}
        </Accordion>
      </CardContent>
    </Card>
  );
}
