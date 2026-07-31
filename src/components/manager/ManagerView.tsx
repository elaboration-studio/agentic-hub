// The unified Manager surface: a left scope rail (Global + workspaces) and a
// content pane. Global scope renders the editable shared matrix (or the empty
// state); workspace scope renders the selected project's read-only inventory.
// The install FAB shows whenever the skills source is enabled: in workspace
// scope it targets the active workspace (the workspace write path); in
// global scope it opens straight into Library scope (the source-root write
// path) since there is no workspace to target.

import { useEffect } from "react";
import { useManagerStore } from "@/state/manager";
import { useWorkspaceStore } from "@/state/workspace";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Matrix } from "@/components/manager/Matrix";
import { EmptyState } from "@/components/manager/EmptyState";
import { ScopeRail } from "@/components/workspace/ScopeRail";
import { InstallFab } from "@/components/install/InstallFab";
import { SuitesPage } from "@/components/suites/SuitesPage";

export function ManagerView() {
  const scope = useManagerStore((s) => s.scope);
  const data = useManagerStore((s) => s.data);
  const readOnly = useManagerStore((s) => s.readOnly);
  const skillsEnabled = useManagerStore((s) => s.data?.settings.skills.enabled ?? false);
  const reload = useWorkspaceStore((s) => s.reload);
  const activeId = useWorkspaceStore((s) => s.activeId);

  // Populate the rail's workspace list on mount, regardless of the default
  // scope. Selecting a workspace (in ScopeRail) loads its inventory; this only
  // fills the list so the rail isn't empty in global scope.
  useEffect(() => {
    void reload();
  }, [reload]);

  return (
    <div className="flex flex-1 gap-5">
      <ScopeRail />
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        {data && data.scanErrors.length > 0 && (
          <details className="rounded-lg border bg-card px-3.5 py-2.5">
            <summary className="cursor-pointer font-semibold text-warning">
              {data.scanErrors.length} scan notice(s)
            </summary>
            <ul className="mt-2.5 list-disc pl-[18px] text-muted-foreground">
              {data.scanErrors.map((err, i) => (
                <li key={i}>
                  <code className="font-mono">{err.path}</code> — {err.message}
                </li>
              ))}
            </ul>
          </details>
        )}
        {scope === "suite" ? (
          <SuitesPage />
        ) : scope === "global" ? (
          !data ? null : data.items.length === 0 ? (
            <EmptyState />
          ) : (
            <Matrix />
          )
        ) : activeId && data && readOnly ? (
          <Matrix />
        ) : (
          <Alert>
            <AlertDescription>
              {activeId
                ? "Loading workspace inventory…"
                : "Select a workspace to see its installed agentic resources."}
            </AlertDescription>
          </Alert>
        )}
      </div>
      {skillsEnabled && scope !== "suite" &&
        (scope === "workspace" ? activeId && <InstallFab workspaceId={activeId} /> : <InstallFab />)}
    </div>
  );
}
