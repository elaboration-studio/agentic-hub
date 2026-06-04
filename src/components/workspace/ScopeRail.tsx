// Unified left rail for the Manager: a pinned **Global** row on top, then the
// remembered project folders. Selecting an entry sets the capability scope —
// Global shows the editable shared matrix, a workspace shows its read-only
// inventory. Switching scope kind resets the scope-specific filters (a source
// id selected in one scope rarely exists in the other). Add a folder, select
// the active one, or remove it. Sticky with inner scrolling.

import { FolderPlus, Globe, X } from "lucide-react";
import { useManagerStore } from "@/state/manager";
import { useManagerFiltersStore } from "@/state/managerFilters";
import { useWorkspaceStore } from "@/state/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function ScopeRail() {
  const scope = useManagerStore((s) => s.scope);
  const setScope = useManagerStore((s) => s.setScope);
  const resetScopedFilters = useManagerFiltersStore((s) => s.resetScopedFilters);

  const targets = useWorkspaceStore((s) => s.targets);
  const activeId = useWorkspaceStore((s) => s.activeId);
  const busy = useWorkspaceStore((s) => s.busy);
  const pick = useWorkspaceStore((s) => s.pick);
  const activate = useWorkspaceStore((s) => s.activate);
  const remove = useWorkspaceStore((s) => s.remove);

  const selectGlobal = () => {
    if (scope === "global") return;
    resetScopedFilters();
    setScope("global");
  };

  const selectWorkspace = (id: string) => {
    if (scope !== "workspace") resetScopedFilters();
    setScope("workspace");
    void activate(id);
  };

  const onAdd = async () => {
    if (scope !== "workspace") resetScopedFilters();
    await pick();
    // pick() activates the new target; reflect that by entering workspace scope.
    if (useWorkspaceStore.getState().activeId) setScope("workspace");
  };

  return (
    <aside className="sticky top-0 flex max-h-[calc(100vh-7.5rem)] w-64 shrink-0 flex-col gap-2.5 self-start">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
          Scope
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => void onAdd()}
          disabled={busy}
          title="Add workspace…"
          aria-label="Add workspace…"
        >
          <FolderPlus />
        </Button>
      </div>

      <ul className="flex flex-col gap-1.5">
        <li
          className={cn(
            "flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2",
            scope === "global" && "border-primary bg-primary/10",
          )}
        >
          <button
            type="button"
            className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left"
            onClick={selectGlobal}
            aria-pressed={scope === "global"}
          >
            <Globe className="size-4 shrink-0 text-muted-foreground" />
            <span className="flex min-w-0 flex-col">
              <span className="truncate font-semibold">Global</span>
              <span className="truncate text-[11px] text-muted-foreground">Shared sources</span>
            </span>
          </button>
        </li>
      </ul>

      <span className="mt-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        Workspaces
      </span>
      {targets.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
          No workspaces yet. Add a project folder to audit its agentic resources.
        </p>
      ) : (
        <ul className="-mr-1 flex flex-col gap-1.5 overflow-y-auto pr-1">
          {targets.map((t) => {
            const active = scope === "workspace" && t.id === activeId;
            return (
              <li
                key={t.id}
                className={cn(
                  "group flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2",
                  active && "border-primary bg-primary/10",
                )}
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 cursor-pointer flex-col items-start text-left"
                  onClick={() => selectWorkspace(t.id)}
                  aria-pressed={active}
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
            );
          })}
        </ul>
      )}
    </aside>
  );
}
