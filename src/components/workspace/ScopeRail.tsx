import { useEffect, useMemo, useState, type ReactNode } from "react";
import { FolderPlus, Globe, Layers3, Plus, Save, X } from "lucide-react";
import type { ToolId } from "@/types";
import { onSuiteStoreChanged } from "@/ipc";
import { enabledTools } from "@/shared";
import { useManagerStore } from "@/state/manager";
import { useManagerFiltersStore } from "@/state/managerFilters";
import { useSuitesStore } from "@/state/suites";
import { useWorkspaceStore } from "@/state/workspace";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

export function ScopeRail() {
  const scope = useManagerStore((state) => state.scope);
  const setScope = useManagerStore((state) => state.setScope);
  const data = useManagerStore((state) => state.data);
  const readOnly = useManagerStore((state) => state.readOnly);
  const refresh = useManagerStore((state) => state.refresh);
  const resetScopedFilters = useManagerFiltersStore((state) => state.resetScopedFilters);

  const suites = useSuitesStore((state) => state.suites);
  const selectedId = useSuitesStore((state) => state.selectedId);
  const isCreating = useSuitesStore((state) => state.isCreating);
  const suiteBusy = useSuitesStore((state) => state.busy);
  const reloadSuites = useSuitesStore((state) => state.reload);
  const selectSuite = useSuitesStore((state) => state.selectSuite);
  const startCreate = useSuitesStore((state) => state.startCreate);
  const startCreateFromCurrent = useSuitesStore((state) => state.startCreateFromCurrent);
  const pruneSelection = useSuitesStore((state) => state.pruneSelection);

  const targets = useWorkspaceStore((state) => state.targets);
  const activeId = useWorkspaceStore((state) => state.activeId);
  const workspaceBusy = useWorkspaceStore((state) => state.busy);
  const pick = useWorkspaceStore((state) => state.pick);
  const activate = useWorkspaceStore((state) => state.activate);
  const remove = useWorkspaceStore((state) => state.remove);

  const createTools = useMemo(() => (data ? enabledTools(data.settings) : []), [data]);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createTool, setCreateTool] = useState<ToolId>(createTools[0]?.id ?? "codex");

  useEffect(() => {
    void reloadSuites();
    const unlisten = onSuiteStoreChanged(() => void reloadSuites());
    return () => void unlisten.then((dispose) => dispose());
  }, [reloadSuites]);

  useEffect(() => {
    pruneSelection();
  }, [suites, pruneSelection]);

  useEffect(() => {
    setCreateTool((current) =>
      createTools.some((tool) => tool.id === current)
        ? current
        : (createTools[0]?.id ?? current),
    );
  }, [createTools]);

  const switchScope = (next: "global" | "suite" | "workspace") => {
    if (scope !== next) resetScopedFilters();
    setScope(next);
  };

  const ensureGlobalData = async () => {
    if (readOnly || !useManagerStore.getState().data) await refresh();
  };

  const selectGlobal = () => switchScope("global");

  const selectSuiteRow = (id: string) => {
    void (async () => {
      switchScope("suite");
      await ensureGlobalData();
      selectSuite(id);
    })();
  };

  const createNew = () => {
    void (async () => {
      switchScope("suite");
      await ensureGlobalData();
      startCreate();
    })();
  };

  const createFromCurrent = () => {
    void (async () => {
      switchScope("suite");
      await ensureGlobalData();
      const manager = useManagerStore.getState();
      if (!manager.data) return;
      startCreateFromCurrent(
        createTool,
        manager.data.items,
        manager.data.result.states,
        manager.readOnlyItemIds,
      );
      setCreateDialogOpen(false);
    })();
  };

  const selectWorkspace = (id: string) => {
    switchScope("workspace");
    void activate(id);
  };

  const addWorkspace = async () => {
    switchScope("workspace");
    await pick();
  };

  return (
    <>
      <aside className="sticky top-0 flex max-h-[calc(100vh-7.5rem)] w-64 shrink-0 flex-col gap-2.5 self-start">
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">Manager</span>
        <ul className="flex flex-col gap-1.5">
          <RailRow active={scope === "global"} onClick={selectGlobal} icon={<Globe />} title="Global" subtitle="Shared sources" />
        </ul>

        <div className="mt-1 flex items-center justify-between">
          <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">Suites</span>
          <Button variant="ghost" size="icon-xs" onClick={createNew} disabled={suiteBusy} title="New suite" aria-label="New suite"><Plus /></Button>
        </div>
        <div className="flex gap-1.5">
          <Button variant="outline" size="sm" className="flex-1 justify-start gap-2" onClick={createNew} disabled={suiteBusy}><Plus />New</Button>
          <Button variant="outline" size="sm" className="flex-1 justify-start gap-2" onClick={() => setCreateDialogOpen(true)} disabled={suiteBusy || createTools.length === 0}><Save />Current</Button>
        </div>
        {suites.length === 0 && !isCreating ? (
          <p className="rounded-lg border border-dashed px-3 py-4 text-center text-sm text-muted-foreground">No suites yet.</p>
        ) : (
          <ul className="-mr-1 flex flex-col gap-1.5 overflow-y-auto pr-1">
            {suites.map((suite) => (
              <li key={suite.id}>
                <button
                  type="button"
                  className={cn("flex w-full items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2 text-left", scope === "suite" && suite.id === selectedId && !isCreating && "border-primary bg-primary/10")}
                  onClick={() => selectSuiteRow(suite.id)}
                  aria-pressed={scope === "suite" && suite.id === selectedId && !isCreating}
                >
                  <Layers3 className="size-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate font-semibold">{suite.name}</span>
                  {suite.isBase && <Badge variant="outline" className="border-primary/40 text-primary">Base</Badge>}
                  <span className="text-[11px] tabular-nums text-muted-foreground">{suite.capabilities.length}</span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="mt-1 flex items-center justify-between">
          <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">Workspaces</span>
          <Button variant="ghost" size="icon-sm" onClick={() => void addWorkspace()} disabled={workspaceBusy} title="Add workspace…" aria-label="Add workspace…"><FolderPlus /></Button>
        </div>
        {targets.length === 0 ? (
          <p className="rounded-lg border border-dashed px-3 py-5 text-center text-sm text-muted-foreground">No workspaces yet.</p>
        ) : (
          <ul className="-mr-1 flex flex-col gap-1.5 overflow-y-auto pr-1">
            {targets.map((target) => {
              const active = scope === "workspace" && target.id === activeId;
              return (
                <li key={target.id} className={cn("group flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2", active && "border-primary bg-primary/10")}>
                  <button type="button" className="flex min-w-0 flex-1 cursor-pointer flex-col items-start text-left" onClick={() => selectWorkspace(target.id)} aria-pressed={active}>
                    <span className="w-full truncate font-semibold">{target.label}</span>
                    <code className="w-full truncate font-mono text-[11px] text-muted-foreground">{target.dir}</code>
                  </button>
                  <Button variant="ghost" size="icon-xs" className="text-destructive opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100 focus-visible:opacity-100" onClick={() => void remove(target.id)} disabled={workspaceBusy} title="Remove workspace" aria-label={`Remove ${target.label}`}><X /></Button>
                </li>
              );
            })}
          </ul>
        )}
      </aside>

      <AlertDialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Create suite from current</AlertDialogTitle>
            <AlertDialogDescription>Choose the tool whose enabled, Hub-managed resources should seed the new suite.</AlertDialogDescription>
          </AlertDialogHeader>
          <Select value={createTool} onValueChange={(value) => setCreateTool(value as ToolId)}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>{createTools.map((tool) => <SelectItem key={tool.id} value={tool.id}>{tool.label}</SelectItem>)}</SelectContent>
          </Select>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={createFromCurrent}>Create draft</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function RailRow(props: { active: boolean; onClick: () => void; icon: ReactNode; title: string; subtitle: string }) {
  return (
    <li className={cn("flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2", props.active && "border-primary bg-primary/10")}>
      <button type="button" className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left" onClick={props.onClick} aria-pressed={props.active}>
        <span className="size-4 shrink-0 text-muted-foreground [&>svg]:size-4">{props.icon}</span>
        <span className="flex min-w-0 flex-col"><span className="truncate font-semibold">{props.title}</span><span className="truncate text-[11px] text-muted-foreground">{props.subtitle}</span></span>
      </button>
    </li>
  );
}
