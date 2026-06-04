// Workspace install: pick a starred skill + target tools, then run the provider
// CLI into the active project. This is the one explicit workspace write; the
// inventory matrix stays read-only and refreshes on the emitted change event.

import { useCallback, useEffect, useState } from "react";
import { Download } from "lucide-react";
import { useSkillsStore } from "@/state/skills";
import { useWorkspaceStore } from "@/state/workspace";
import { WORKSPACE_TOOLS } from "@/shared";
import type { ToolId } from "@/types";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const DEFAULT_TOOLS = WORKSPACE_TOOLS.map((t) => t.id);

export function InstallSkillDialog() {
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tools, setTools] = useState<ToolId[]>(DEFAULT_TOOLS);

  const favorites = useSkillsStore((s) => s.favorites);
  const installing = useSkillsStore((s) => s.installing);
  const install = useSkillsStore((s) => s.install);
  const loadFavorites = useSkillsStore((s) => s.loadFavorites);
  const activeId = useWorkspaceStore((s) => s.activeId);

  useEffect(() => {
    if (open) void loadFavorites();
  }, [open, loadFavorites]);

  const toggleTool = useCallback((id: ToolId, on: boolean) => {
    setTools((prev) => (on ? [...new Set([...prev, id])] : prev.filter((t) => t !== id)));
  }, []);

  const onInstall = useCallback(async () => {
    const fav = favorites.find((f) => f.id === selectedId);
    if (!fav || !activeId) return;
    const result = await install(fav, activeId, tools);
    if (result?.ok) setOpen(false);
  }, [favorites, selectedId, activeId, tools, install]);

  const canInstall = selectedId !== null && tools.length > 0 && !installing;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" disabled={!activeId}>
          <Download className="size-4" />
          Install skill…
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Install a starred skill</DialogTitle>
          <DialogDescription>
            Pick a skill and the tools to install it for. The skill is added to this
            project via its source CLI; the inventory refreshes when it lands.
          </DialogDescription>
        </DialogHeader>

        {favorites.length === 0 ? (
          <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
            No starred skills. Star some on the Skills page first.
          </p>
        ) : (
          <div className="flex flex-col gap-3">
            <ul className="flex max-h-56 flex-col gap-1.5 overflow-auto">
              {favorites.map((f) => (
                <li key={`${f.provider}:${f.id}`}>
                  <button
                    type="button"
                    className={cn(
                      "flex w-full flex-col items-start rounded-lg border bg-secondary px-3 py-2 text-left",
                      f.id === selectedId && "border-primary bg-primary/10",
                    )}
                    onClick={() => setSelectedId(f.id)}
                    aria-pressed={f.id === selectedId}
                  >
                    <span className="truncate font-semibold">{f.name}</span>
                    <code className="truncate font-mono text-[11px] text-muted-foreground">
                      {f.installRef}
                    </code>
                  </button>
                </li>
              ))}
            </ul>

            <div className="flex flex-col gap-1.5">
              <span className="text-xs font-medium">Target tools</span>
              <div className="flex flex-wrap gap-3">
                {WORKSPACE_TOOLS.map((t) => (
                  <Label key={t.id} className="flex cursor-pointer items-center gap-2">
                    <Checkbox
                      checked={tools.includes(t.id)}
                      onCheckedChange={(v) => toggleTool(t.id, v === true)}
                    />
                    <span>{t.label}</span>
                  </Label>
                ))}
              </div>
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => setOpen(false)} disabled={installing}>
            Cancel
          </Button>
          <Button onClick={() => void onInstall()} disabled={!canInstall}>
            {installing ? "Installing…" : "Install"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
