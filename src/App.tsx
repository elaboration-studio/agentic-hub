import { useEffect, useState } from "react";
import { onApplyProgress, onHubNavigate, onMenuOpenConfig, onSourcesChanged } from "./ipc";
import { useManagerStore } from "./state/manager";
import type { Route } from "./shared";
import { Header } from "./components/layout/Header";
import { ActionBar } from "./components/layout/ActionBar";
import { Matrix } from "./components/manager/Matrix";
import { EmptyState } from "./components/manager/EmptyState";
import { ConflictDialog } from "./components/manager/ConflictDialog";
import { ConfigPage } from "./components/config/ConfigPage";
import { SuitesPage } from "./components/suites/SuitesPage";
import { WorkspacePanel } from "./components/workspace/WorkspacePanel";
import { Alert, AlertDescription } from "./components/ui/alert";
import { Toaster } from "./components/ui/sonner";
import { TooltipProvider } from "./components/ui/tooltip";

const ROUTES: Route[] = ["manager", "suites", "config"];

function routeFromHash(): Route {
  const hash = window.location.hash.replace(/^#\/?/, "") as Route;
  return ROUTES.includes(hash) ? hash : "manager";
}

function navigate(route: Route) {
  window.location.hash = route === "manager" ? "" : `#/${route}`;
}

export function App() {
  const status = useManagerStore((s) => s.status);
  const error = useManagerStore((s) => s.error);
  const data = useManagerStore((s) => s.data);
  const scope = useManagerStore((s) => s.scope);
  const pending = useManagerStore((s) => s.pendingKeys.length);
  const refresh = useManagerStore((s) => s.refresh);
  const setProgress = useManagerStore((s) => s.setProgress);

  const [route, setRoute] = useState<Route>(routeFromHash);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const onHash = () => setRoute(routeFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    const unlisten = onApplyProgress((e) =>
      setProgress({ done: e.operationIndex + 1, total: e.totalOperations }),
    );
    return () => void unlisten.then((fn) => fn());
  }, [setProgress]);

  // Cross-window navigation: the Settings menu item (Cmd+,) and palette nav
  // commands route the main window to a route.
  useEffect(() => {
    const config = onMenuOpenConfig(() => navigate("config"));
    const nav = onHubNavigate((route) => navigate(route));
    return () => {
      void config.then((fn) => fn());
      void nav.then((fn) => fn());
    };
  }, []);

  // Live-refresh on watcher / resync events. Skip while the user has unapplied
  // edits so an incoming event never discards an in-progress selection.
  useEffect(() => {
    const unlisten = onSourcesChanged(() => {
      if (useManagerStore.getState().pendingKeys.length === 0) void refresh();
    });
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  return (
    <TooltipProvider>
      <div className="flex h-full flex-col">
        <Header route={route} onNavigate={navigate} />
        <main className="flex flex-1 flex-col gap-[18px] overflow-auto px-6 pb-12 pt-5">
          {status === "error" && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          {status === "loading" && !data && (
            <Alert>
              <AlertDescription>Scanning sources…</AlertDescription>
            </Alert>
          )}
          {data && route === "config" && <ConfigPage />}
          {data && route === "suites" && <SuitesPage />}
          {data && route === "manager" && (
            <>
              {data.scanErrors.length > 0 && (
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
              {scope === "global" ? (
                data.items.length === 0 ? (
                  <EmptyState />
                ) : (
                  <Matrix />
                )
              ) : (
                <WorkspacePanel />
              )}
            </>
          )}
        </main>
        {route === "manager" && scope === "global" && pending > 0 && <ActionBar />}
        <ConflictDialog />
      </div>
      <Toaster />
    </TooltipProvider>
  );
}
