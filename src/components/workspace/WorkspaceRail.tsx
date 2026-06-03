// Left rail for workspace scope: the remembered project folders. Add a folder,
// select the active one (loads its read-only inventory), or remove it.

import { FolderPlus, X } from "lucide-react";
import { useWorkspaceStore } from "@/state/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function WorkspaceRail() {
  const targets = useWorkspaceStore((s) => s.targets);
  const activeId = useWorkspaceStore((s) => s.activeId);
  const busy = useWorkspaceStore((s) => s.busy);
  const pick = useWorkspaceStore((s) => s.pick);
  const activate = useWorkspaceStore((s) => s.activate);
  const remove = useWorkspaceStore((s) => s.remove);

  return (
    <aside className="flex w-64 shrink-0 flex-col gap-2.5">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
          Workspaces
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => void pick()}
          disabled={busy}
          title="Add workspace…"
          aria-label="Add workspace…"
        >
          <FolderPlus />
        </Button>
      </div>
      {targets.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
          No workspaces yet. Add a project folder to audit its agentic resources.
        </p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {targets.map((t) => (
            <li
              key={t.id}
              className={cn(
                "group flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2",
                t.id === activeId && "border-primary bg-primary/10",
              )}
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 cursor-pointer flex-col items-start text-left"
                onClick={() => void activate(t.id)}
                aria-pressed={t.id === activeId}
              >
                <span className="w-full truncate font-semibold">{t.label}</span>
                <code className="w-full truncate font-mono text-[11px] text-muted-foreground">
                  {t.dir}
                </code>
              </button>
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-destructive opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100 focus-visible:opacity-100"
                onClick={() => void remove(t.id)}
                disabled={busy}
                title="Remove workspace"
                aria-label={`Remove ${t.label}`}
              >
                <X className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}
