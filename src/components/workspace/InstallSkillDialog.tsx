// Workspace install: pick starred skills and, per skill, the tools to target,
// then run the provider CLI into the active project. This is the one explicit
// workspace write; the inventory matrix stays read-only and refreshes on the
// emitted change event. The trigger is a floating action button so it never
// crowds the matrix toolbar; the picker is a skill × tool matrix so several
// skills can be installed at once (and future resource sources can reuse it).

import { useCallback, useEffect, useState } from "react";
import { Info, Plus } from "lucide-react";
import { useSkillsStore, type InstallItem } from "@/state/skills";
import { useWorkspaceStore } from "@/state/workspace";
import { WORKSPACE_TOOLS } from "@/shared";
import type { ToolId } from "@/types";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

// Selection state: favorite id → the tools to install it for. A skill with no
// tools is simply absent, so the keys are exactly the skills to install.
type Picks = Record<string, ToolId[]>;

function withTool(tools: ToolId[] | undefined, tool: ToolId, on: boolean): ToolId[] {
  const set = new Set(tools);
  if (on) set.add(tool);
  else set.delete(tool);
  return [...set];
}

export function InstallSkillDialog() {
  const [open, setOpen] = useState(false);
  const [picks, setPicks] = useState<Picks>({});

  const favorites = useSkillsStore((s) => s.favorites);
  const installing = useSkillsStore((s) => s.installing);
  const installLog = useSkillsStore((s) => s.installLog);
  const installMany = useSkillsStore((s) => s.installMany);
  const clearInstallLog = useSkillsStore((s) => s.clearInstallLog);
  const loadFavorites = useSkillsStore((s) => s.loadFavorites);
  const activeId = useWorkspaceStore((s) => s.activeId);

  // Refresh the starred list and clear prior selection + output each time it opens.
  useEffect(() => {
    if (open) {
      void loadFavorites();
      setPicks({});
      clearInstallLog();
    }
  }, [open, loadFavorites, clearInstallLog]);

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

  const columnState = useCallback(
    (tool: ToolId): boolean | "indeterminate" => {
      if (favorites.length === 0) return false;
      const on = favorites.filter((f) => (picks[f.id] ?? []).includes(tool)).length;
      if (on === 0) return false;
      return on === favorites.length ? true : "indeterminate";
    },
    [favorites, picks],
  );

  const items: InstallItem[] = favorites.flatMap((f) => {
    const toolIds = picks[f.id];
    return toolIds?.length ? [{ favorite: f, toolIds }] : [];
  });

  const onInstall = useCallback(async () => {
    if (!activeId || items.length === 0) return;
    const { failed } = await installMany(items, activeId);
    if (failed === 0) setOpen(false);
  }, [activeId, items, installMany]);

  const canInstall = items.length > 0 && !installing;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          size="icon-lg"
          className="fixed bottom-6 right-6 z-30 size-14 rounded-full shadow-lg"
          disabled={!activeId}
          title="Install skill…"
          aria-label="Install skill…"
        >
          <Plus className="size-6" />
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Install starred skills</DialogTitle>
          <DialogDescription>
            Tick the tools to install each skill for, then Install. Skills are added
            to this project via their source CLI; the inventory refreshes when they
            land.
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-start gap-2 rounded-lg border bg-secondary/40 px-3 py-2 text-xs text-muted-foreground">
          <Info className="mt-0.5 size-3.5 shrink-0" />
          <span>
            Installs run <code className="font-mono">npx skills add</code>, so they
            need Node.js (npx) on your PATH. Verify in{" "}
            <span className="font-medium">Config → skills.sh → Check CLI</span>.
          </span>
        </div>

        {favorites.length === 0 ? (
          <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
            No starred skills. Star some on the Resources page first.
          </p>
        ) : (
          <div className="max-h-88 overflow-auto rounded-lg border">
            <table className="w-full text-sm">
              <thead className="sticky top-0 z-10 bg-secondary/80 backdrop-blur">
                <tr className="border-b text-left">
                  <th className="px-3 py-2 font-medium">Skill</th>
                  {WORKSPACE_TOOLS.map((t) => (
                    <th key={t.id} className="px-3 py-2 text-center font-medium">
                      <label className="flex cursor-pointer flex-col items-center gap-1">
                        <span>{t.label}</span>
                        <Checkbox
                          checked={columnState(t.id)}
                          onCheckedChange={(v) => toggleColumn(t.id, v === true)}
                          aria-label={`Install all skills for ${t.label}`}
                        />
                      </label>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {favorites.map((f) => (
                  <tr key={`${f.provider}:${f.id}`} className="border-b last:border-0">
                    <td className="px-3 py-2">
                      <div className="flex min-w-0 flex-col">
                        <span className="truncate font-semibold">{f.name}</span>
                        <code className="truncate font-mono text-[11px] text-muted-foreground">
                          {f.installRef}
                        </code>
                      </div>
                    </td>
                    {WORKSPACE_TOOLS.map((t) => (
                      <td key={t.id} className="px-3 py-2 text-center">
                        <Checkbox
                          checked={(picks[f.id] ?? []).includes(t.id)}
                          onCheckedChange={(v) => toggleCell(f.id, t.id, v === true)}
                          aria-label={`Install ${f.name} for ${t.label}`}
                        />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {installLog && (
          <div className="flex flex-col gap-1">
            <span className="text-xs font-medium text-muted-foreground">Output</span>
            <pre className="max-h-40 overflow-auto rounded-lg border bg-secondary/40 px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap wrap-break-word">
              {installLog}
            </pre>
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => setOpen(false)} disabled={installing}>
            Cancel
          </Button>
          <Button onClick={() => void onInstall()} disabled={!canInstall}>
            {installing
              ? "Installing…"
              : items.length > 1
                ? `Install ${items.length} skills`
                : "Install"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
