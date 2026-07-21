// The dedicated install window (label=install). Loads the starred-skill matrix
// for the workspace it was opened against, runs `npx skills add` per selected
// skill while streaming output live into an auto-scrolling console, and exposes
// a Cancel that kills the running process. Selection + console live together so
// the whole "install" flow is one cohesive surface.
//
// Two scopes (a Workspace | Library toggle): Workspace installs into the
// project the window was opened for (the original path); Library installs
// into a Hub source root's contract layout, so one install is projectable
// across every tool and project.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Info, RefreshCw, Search } from "lucide-react";
import {
  cancelInstall,
  installSkillStream,
  onInstallContextChanged,
  resolveSources,
  takeInstallContext,
  updateSkillStream,
} from "@/ipc";
import { useSkillsStore, filterFavorites } from "@/state/skills";
import { messageOf } from "@/shared";
import type { InstallContext, InstallScope, SkillInstallEvent, SourceConfig, ToolId } from "@/types";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { InstallMatrix, type InstallStatus } from "./InstallMatrix";

// Workspace scope selection: favorite id → the tools to install it for. A
// skill with no tools is absent, so the keys are exactly the skills to install.
type Picks = Record<string, ToolId[]>;

function withTool(tools: ToolId[] | undefined, tool: ToolId, on: boolean): ToolId[] {
  const set = new Set(tools);
  if (on) set.add(tool);
  else set.delete(tool);
  return [...set];
}

export function InstallWindow() {
  const favorites = useSkillsStore((s) => s.favorites);
  const loadFavorites = useSkillsStore((s) => s.loadFavorites);

  const [context, setContext] = useState<InstallContext | null>(null);
  const [scope, setScope] = useState<InstallScope>("workspace");
  const [picks, setPicks] = useState<Picks>({});
  const [libraryPicks, setLibraryPicks] = useState<Set<string>>(new Set());
  const [sources, setSources] = useState<SourceConfig[]>([]);
  const [sourceId, setSourceId] = useState<string>("");
  const [destSubpath, setDestSubpath] = useState("");
  const [statuses, setStatuses] = useState<Record<string, InstallStatus>>({});
  const [lines, setLines] = useState<string[]>([]);
  const [installing, setInstalling] = useState(false);
  const [filter, setFilter] = useState("");
  const cancelledRef = useRef(false);
  const consoleRef = useRef<HTMLPreElement>(null);

  // Selection always spans the full starred set, so a filter narrows only what
  // is shown — it never drops already-picked (now-hidden) skills.
  const shown = useMemo(() => filterFavorites(favorites, filter), [favorites, filter]);

  const load = useCallback(async () => {
    try {
      const ctx = await takeInstallContext();
      setContext(ctx);
      setScope(ctx.update?.scope ?? "workspace");
    } catch {
      // No context (window opened out of band) — leave null; the UI guides.
    }
    await loadFavorites();
    try {
      const list = await resolveSources();
      setSources(list);
      setSourceId((cur) => (cur && list.some((s) => s.id === cur) ? cur : (list[0]?.id ?? "")));
    } catch {
      // Library scope simply has nothing to pick from; install stays disabled.
    }
  }, [loadFavorites]);

  useEffect(() => {
    void load();
    const unlisten = onInstallContextChanged(() => void load());
    return () => void unlisten.then((fn) => fn());
  }, [load]);

  // Auto-scroll the console to the newest line.
  useEffect(() => {
    const el = consoleRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const appendLine = useCallback((text: string) => {
    setLines((prev) => [...prev, text]);
  }, []);

  const setStatus = useCallback((id: string, status: InstallStatus) => {
    setStatuses((prev) => ({ ...prev, [id]: status }));
  }, []);

  const toggleCell = useCallback((favId: string, tool: ToolId, on: boolean) => {
    setPicks((prev) => {
      const next = withTool(prev[favId], tool, on);
      const copy = { ...prev };
      if (next.length) copy[favId] = next;
      else delete copy[favId];
      return copy;
    });
  }, []);

  // Column "select all" applies to the visible (filtered) rows only, while
  // leaving any hidden rows' picks untouched.
  const toggleColumn = useCallback(
    (tool: ToolId, on: boolean) => {
      setPicks((prev) => {
        const copy: Picks = { ...prev };
        for (const f of shown) {
          const next = withTool(copy[f.id], tool, on);
          if (next.length) copy[f.id] = next;
          else delete copy[f.id];
        }
        return copy;
      });
    },
    [shown],
  );

  const toggleLibraryPick = useCallback((favId: string, on: boolean) => {
    setLibraryPicks((prev) => {
      const next = new Set(prev);
      if (on) next.add(favId);
      else next.delete(favId);
      return next;
    });
  }, []);

  // Run one workspace-scope skill install, resolving when its stream ends.
  // Lines stream into the console; the terminal `done` carries the outcome.
  const runOne = useCallback(
    (
      provider: string,
      installRef: string,
      slug: string,
      workspaceId: string,
      toolIds: ToolId[],
    ) =>
      new Promise<{ ok: boolean; cancelled: boolean }>((resolve) => {
        let settled = false;
        const channel = new Channel<SkillInstallEvent>();
        channel.onmessage = (ev) => {
          if (ev.kind === "line") appendLine(ev.text);
          else {
            settled = true;
            resolve({ ok: ev.ok, cancelled: ev.cancelled });
          }
        };
        installSkillStream(
          { provider, installRef, scope: "workspace", workspaceId, slug, toolIds },
          channel,
        ).catch((e) => {
          appendLine(messageOf(e));
          if (!settled) resolve({ ok: false, cancelled: false });
        });
      }),
    [appendLine],
  );

  // Run one library-scope skill install into the selected source root.
  const runOneLibrary = useCallback(
    (provider: string, installRef: string, slug: string) =>
      new Promise<{ ok: boolean; cancelled: boolean }>((resolve) => {
        let settled = false;
        const channel = new Channel<SkillInstallEvent>();
        channel.onmessage = (ev) => {
          if (ev.kind === "line") appendLine(ev.text);
          else {
            settled = true;
            resolve({ ok: ev.ok, cancelled: ev.cancelled });
          }
        };
        installSkillStream(
          { provider, installRef, scope: "library", sourceId, destSubpath, slug, toolIds: [] },
          channel,
        ).catch((e) => {
          appendLine(messageOf(e));
          if (!settled) resolve({ ok: false, cancelled: false });
        });
      }),
    [appendLine, sourceId, destSubpath],
  );

  // Update mode runs a single re-install for the locked skill the window was
  // opened against. Reuses the same streaming console + Cancel.
  const onUpdate = useCallback(async () => {
    if (!context?.update) return;
    const update = context.update;
    cancelledRef.current = false;
    setInstalling(true);
    setLines([]);
    await new Promise<void>((resolve) => {
      let settled = false;
      const channel = new Channel<SkillInstallEvent>();
      channel.onmessage = (ev) => {
        if (ev.kind === "line") appendLine(ev.text);
        else {
          settled = true;
          resolve();
        }
      };
      const payload =
        update.scope === "library"
          ? {
              provider: update.provider,
              scope: "library" as const,
              name: update.name,
              sourceId: update.sourceId ?? undefined,
              installRef: update.installRef,
              slug: update.slug ?? undefined,
              destSubpath: update.destSubpath ?? undefined,
            }
          : {
              provider: update.provider,
              scope: "workspace" as const,
              name: update.name,
              workspaceId: context.workspaceId ?? undefined,
            };
      updateSkillStream(payload, channel).catch((e) => {
        appendLine(messageOf(e));
        if (!settled) resolve();
      });
    });
    setInstalling(false);
  }, [context, appendLine]);

  const onInstall = useCallback(async () => {
    if (!context) return;
    cancelledRef.current = false;
    setInstalling(true);
    setLines([]);

    if (scope === "workspace") {
      if (!context.workspaceId) return;
      const items = favorites.flatMap((f) => {
        const toolIds = picks[f.id];
        return toolIds?.length ? [{ fav: f, toolIds }] : [];
      });
      if (items.length === 0) {
        setInstalling(false);
        return;
      }
      setStatuses(Object.fromEntries(items.map((i) => [i.fav.id, "idle" as InstallStatus])));
      try {
        for (const { fav, toolIds } of items) {
          if (cancelledRef.current) {
            setStatus(fav.id, "cancelled");
            continue;
          }
          setStatus(fav.id, "running");
          const res = await runOne(fav.provider, fav.installRef, fav.slug, context.workspaceId, toolIds);
          setStatus(fav.id, res.cancelled ? "cancelled" : res.ok ? "ok" : "failed");
          if (res.cancelled) cancelledRef.current = true;
        }
      } finally {
        setInstalling(false);
      }
      return;
    }

    // Library scope: no tool columns — just the picked skills, into one source.
    const items = favorites.filter((f) => libraryPicks.has(f.id));
    if (items.length === 0 || !sourceId) {
      setInstalling(false);
      return;
    }
    setStatuses(Object.fromEntries(items.map((f) => [f.id, "idle" as InstallStatus])));
    try {
      for (const fav of items) {
        if (cancelledRef.current) {
          setStatus(fav.id, "cancelled");
          continue;
        }
        setStatus(fav.id, "running");
        const res = await runOneLibrary(fav.provider, fav.installRef, fav.slug);
        setStatus(fav.id, res.cancelled ? "cancelled" : res.ok ? "ok" : "failed");
        if (res.cancelled) cancelledRef.current = true;
      }
    } finally {
      setInstalling(false);
    }
  }, [context, scope, favorites, picks, libraryPicks, sourceId, runOne, runOneLibrary, setStatus]);

  const onCancel = useCallback(async () => {
    cancelledRef.current = true;
    try {
      await cancelInstall();
    } catch {
      // Best-effort: the stream still terminates on its own.
    }
  }, []);

  const update = context?.update ?? null;
  const selectedCount =
    scope === "workspace"
      ? favorites.filter((f) => (picks[f.id] ?? []).length > 0).length
      : libraryPicks.size;
  const canInstall =
    selectedCount > 0 && !installing && context !== null && (scope === "workspace" || sourceId !== "");
  const sourceLabel = sources.find((s) => s.id === sourceId)?.label;
  const updateSourceLabel = update?.sourceId
    ? sources.find((s) => s.id === update.sourceId)?.label
    : undefined;

  return (
    <div className="flex h-full flex-col gap-3 bg-background p-5 text-foreground">
      <header className="flex items-start justify-between gap-3">
        <div>
          <h1 className="text-base font-semibold tracking-[0.2px]">
            {update ? "Update skill" : "Install skills"}
          </h1>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {update ? (
              update.scope === "library" ? (
                <>
                  in <span className="font-medium text-foreground">{updateSourceLabel ?? "your library"}</span>
                </>
              ) : (
                <>
                  in <span className="font-medium text-foreground">{context?.workspaceLabel}</span>
                </>
              )
            ) : context ? (
              scope === "workspace" ? (
                <>
                  into <span className="font-medium text-foreground">{context.workspaceLabel}</span>
                </>
              ) : (
                <>
                  into <span className="font-medium text-foreground">{sourceLabel ?? "your library"}</span>
                </>
              )
            ) : (
              "No workspace context — reopen from a workspace."
            )}
          </p>
        </div>
        {!update && context && (
          <ToggleGroup
            type="single"
            value={scope}
            onValueChange={(v) => v && setScope(v as InstallScope)}
            variant="outline"
            disabled={installing}
          >
            <ToggleGroupItem value="workspace" aria-label="Install into this workspace">
              Workspace
            </ToggleGroupItem>
            <ToggleGroupItem value="library" aria-label="Install into your library">
              Library
            </ToggleGroupItem>
          </ToggleGroup>
        )}
      </header>

      <div className="flex items-start gap-2 rounded-lg border bg-secondary/40 px-3 py-2 text-xs text-muted-foreground">
        <Info className="mt-0.5 size-3.5 shrink-0" />
        <span>
          {update ? (
            <>
              Updates run <code className="font-mono">npx skills</code>, so they need Node.js (npx) on
              your PATH. Verify in <span className="font-medium">Config → skills.sh → Check CLI</span>.
            </>
          ) : (
            <>
              Installs run <code className="font-mono">npx skills add</code>, so they need Node.js
              (npx) on your PATH. Verify in{" "}
              <span className="font-medium">Config → skills.sh → Check CLI</span>.
            </>
          )}
        </span>
      </div>

      {!update && scope === "library" && (
        <div className="flex shrink-0 items-end gap-2">
          <div className="flex-1 space-y-1">
            <span className="text-xs font-medium text-muted-foreground">Source</span>
            <Select value={sourceId} onValueChange={setSourceId} disabled={installing || sources.length === 0}>
              <SelectTrigger>
                <SelectValue placeholder="No sources configured" />
              </SelectTrigger>
              <SelectContent>
                {sources.map((s) => (
                  <SelectItem key={s.id} value={s.id}>
                    {s.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex-1 space-y-1">
            <span className="text-xs font-medium text-muted-foreground">Destination under skills/</span>
            <Input
              value={destSubpath}
              placeholder="e.g. arno/cmo (optional)"
              disabled={installing}
              onChange={(e) => setDestSubpath(e.target.value)}
            />
          </div>
        </div>
      )}

      {update ? (
        <div className="shrink-0 rounded-lg border px-3 py-3">
          <div className="flex items-center gap-2">
            <RefreshCw className="size-4 text-muted-foreground" />
            <span className="font-semibold">{update.name}</span>
          </div>
          <p className="mt-1 font-mono text-[11px] text-muted-foreground">{update.installRef}</p>
        </div>
      ) : favorites.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
          No starred skills. Star some on the Resources page first.
        </p>
      ) : (
        <>
          <div className="relative shrink-0">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={filter}
              placeholder="Filter starred skills…"
              className="pl-9"
              disabled={installing}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>
          {shown.length === 0 ? (
            <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
              No starred skills match “{filter.trim()}”.
            </p>
          ) : scope === "workspace" ? (
            <div className="max-h-72 shrink-0 overflow-auto">
              <InstallMatrix
                favorites={shown}
                picks={picks}
                statuses={statuses}
                disabled={installing}
                onToggleCell={toggleCell}
                onToggleColumn={toggleColumn}
              />
            </div>
          ) : (
            <div className="max-h-72 shrink-0 overflow-auto rounded-lg border">
              <ul className="divide-y">
                {shown.map((f) => (
                  <li key={`${f.provider}:${f.id}`} className="flex items-center gap-3 px-3 py-2">
                    <Checkbox
                      checked={libraryPicks.has(f.id)}
                      disabled={installing}
                      onCheckedChange={(v) => toggleLibraryPick(f.id, v === true)}
                      aria-label={`Install ${f.name} into the library`}
                    />
                    <div className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-semibold">{f.name}</span>
                      <code className="truncate font-mono text-[11px] text-muted-foreground">
                        {f.installRef}
                      </code>
                    </div>
                    {statuses[f.id] === "running" && (
                      <RefreshCw className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </>
      )}

      {(lines.length > 0 || installing) && (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">Output</span>
          <pre
            ref={consoleRef}
            className="min-h-0 flex-1 overflow-auto rounded-lg border bg-secondary/40 px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap wrap-break-word"
          >
            {lines.join("\n")}
          </pre>
        </div>
      )}

      <footer className="flex shrink-0 justify-end gap-2">
        <Button variant="ghost" onClick={() => void getCurrentWindow().close()} disabled={installing}>
          Close
        </Button>
        {installing ? (
          <Button variant="destructive" onClick={() => void onCancel()}>
            Cancel
          </Button>
        ) : update ? (
          <Button onClick={() => void onUpdate()} disabled={context === null}>
            Update
          </Button>
        ) : (
          <Button onClick={() => void onInstall()} disabled={!canInstall}>
            {selectedCount > 1 ? `Install ${selectedCount} skills` : "Install"}
          </Button>
        )}
      </footer>
    </div>
  );
}
