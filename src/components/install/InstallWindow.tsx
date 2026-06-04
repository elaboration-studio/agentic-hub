// The dedicated install window (label=install). Loads the starred-skill matrix
// for the workspace it was opened against, runs `npx skills add` per selected
// skill while streaming output live into an auto-scrolling console, and exposes
// a Cancel that kills the running process. Selection + console live together so
// the whole "install" flow is one cohesive surface.

import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Info } from "lucide-react";
import {
  cancelInstall,
  installSkillStream,
  onInstallContextChanged,
  takeInstallContext,
} from "@/ipc";
import { useSkillsStore } from "@/state/skills";
import { messageOf } from "@/shared";
import type { InstallContext, SkillInstallEvent, ToolId } from "@/types";
import { Button } from "@/components/ui/button";
import { InstallMatrix, type InstallStatus } from "./InstallMatrix";

// Selection: favorite id → the tools to install it for. A skill with no tools
// is absent, so the keys are exactly the skills to install.
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
  const [picks, setPicks] = useState<Picks>({});
  const [statuses, setStatuses] = useState<Record<string, InstallStatus>>({});
  const [lines, setLines] = useState<string[]>([]);
  const [installing, setInstalling] = useState(false);
  const cancelledRef = useRef(false);
  const consoleRef = useRef<HTMLPreElement>(null);

  const load = useCallback(async () => {
    try {
      setContext(await takeInstallContext());
    } catch {
      // No context (window opened out of band) — leave null; the UI guides.
    }
    await loadFavorites();
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

  const toggleColumn = useCallback(
    (tool: ToolId, on: boolean) => {
      setPicks((prev) => {
        const copy: Picks = {};
        for (const f of favorites) {
          const next = withTool(prev[f.id], tool, on);
          if (next.length) copy[f.id] = next;
        }
        return copy;
      });
    },
    [favorites],
  );

  // Run one skill install, resolving when its stream ends. Lines stream into the
  // console; the terminal `done` carries the outcome.
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
        installSkillStream({ provider, installRef, slug, workspaceId, toolIds }, channel).catch(
          (e) => {
            appendLine(messageOf(e));
            if (!settled) resolve({ ok: false, cancelled: false });
          },
        );
      }),
    [appendLine],
  );

  const onInstall = useCallback(async () => {
    if (!context) return;
    const items = favorites.flatMap((f) => {
      const toolIds = picks[f.id];
      return toolIds?.length ? [{ fav: f, toolIds }] : [];
    });
    if (items.length === 0) return;

    cancelledRef.current = false;
    setInstalling(true);
    setLines([]);
    setStatuses(Object.fromEntries(items.map((i) => [i.fav.id, "idle" as InstallStatus])));
    try {
      for (const { fav, toolIds } of items) {
        if (cancelledRef.current) {
          setStatus(fav.id, "cancelled");
          continue;
        }
        setStatus(fav.id, "running");
        const res = await runOne(
          fav.provider,
          fav.installRef,
          fav.slug,
          context.workspaceId,
          toolIds,
        );
        setStatus(fav.id, res.cancelled ? "cancelled" : res.ok ? "ok" : "failed");
        if (res.cancelled) cancelledRef.current = true;
      }
    } finally {
      setInstalling(false);
    }
  }, [context, favorites, picks, runOne, setStatus]);

  const onCancel = useCallback(async () => {
    cancelledRef.current = true;
    try {
      await cancelInstall();
    } catch {
      // Best-effort: the stream still terminates on its own.
    }
  }, []);

  const selectedCount = favorites.filter((f) => (picks[f.id] ?? []).length > 0).length;
  const canInstall = selectedCount > 0 && !installing && context !== null;

  return (
    <div className="flex h-full flex-col gap-3 bg-background p-5 text-foreground">
      <header>
        <h1 className="text-base font-semibold tracking-[0.2px]">Install skills</h1>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {context ? (
            <>
              into <span className="font-medium text-foreground">{context.workspaceLabel}</span>
            </>
          ) : (
            "No workspace context — reopen from a workspace."
          )}
        </p>
      </header>

      <div className="flex items-start gap-2 rounded-lg border bg-secondary/40 px-3 py-2 text-xs text-muted-foreground">
        <Info className="mt-0.5 size-3.5 shrink-0" />
        <span>
          Installs run <code className="font-mono">npx skills add</code>, so they need Node.js (npx)
          on your PATH. Verify in <span className="font-medium">Config → skills.sh → Check CLI</span>.
        </span>
      </div>

      {favorites.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
          No starred skills. Star some on the Resources page first.
        </p>
      ) : (
        <div className="max-h-72 shrink-0 overflow-auto">
          <InstallMatrix
            favorites={favorites}
            picks={picks}
            statuses={statuses}
            disabled={installing}
            onToggleCell={toggleCell}
            onToggleColumn={toggleColumn}
          />
        </div>
      )}

      {(lines.length > 0 || installing) && (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">Output</span>
          <pre
            ref={consoleRef}
            className="min-h-32 flex-1 overflow-auto rounded-lg border bg-secondary/40 px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap wrap-break-word"
          >
            {lines.join("\n")}
          </pre>
        </div>
      )}

      <footer className="flex justify-end gap-2">
        <Button variant="ghost" onClick={() => void getCurrentWindow().close()} disabled={installing}>
          Close
        </Button>
        {installing ? (
          <Button variant="destructive" onClick={() => void onCancel()}>
            Cancel
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
