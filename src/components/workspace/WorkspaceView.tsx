// Workspace scope: a two-pane read-only audit. Left rail picks a project; the
// right pane reuses the global manager matrix to show which agentic resources
// each tool (Codex / Claude / Cursor) already has installed in that project.

import { useEffect } from "react";
import { useManagerStore } from "@/state/manager";
import { useWorkspaceStore } from "@/state/workspace";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Matrix } from "@/components/manager/Matrix";
import { WorkspaceRail } from "./WorkspaceRail";

export function WorkspaceView() {
  const activeId = useWorkspaceStore((s) => s.activeId);
  const reload = useWorkspaceStore((s) => s.reload);
  const data = useManagerStore((s) => s.data);
  const readOnly = useManagerStore((s) => s.readOnly);
  const loadWorkspace = useManagerStore((s) => s.loadWorkspace);

  // On entering workspace scope: load remembered targets, then the active one's
  // inventory. Self-contained so the view works regardless of how it mounts.
  useEffect(() => {
    void (async () => {
      await reload();
      const id = useWorkspaceStore.getState().activeId;
      if (id) await loadWorkspace(id);
    })();
  }, [reload, loadWorkspace]);

  return (
    <div className="flex flex-1 gap-5">
      <WorkspaceRail />
      <div className="min-w-0 flex-1">
        {activeId && data && readOnly ? (
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
    </div>
  );
}
