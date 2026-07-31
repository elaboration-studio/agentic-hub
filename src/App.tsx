import { useEffect, useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { toast } from "sonner";
import {
  onAppReopened,
  onApplyProgress,
  onHubLocate,
  onHubNavigate,
  onHubWatcherChanged,
  onMenuCheckUpdates,
  onMenuOpenConfig,
  onSourcesChanged,
  onUsageTracingHealthFailed,
  onWorkspaceChanged,
  restartApp,
} from "./ipc";
import type { UsageTracingHealthFailure } from "./types";
import { useManagerStore } from "./state/manager";
import { UPDATE_CHECK_INTERVAL_MS, useUpdateStore } from "./state/update";
import { useManagerFiltersStore } from "./state/managerFilters";
import { useWorkspaceStore } from "./state/workspace";
import { enterSuiteManagerScope } from "./state/scopeNavigation";
import { WORKSPACE_ID_PREFIX, type Route } from "./shared";
import { Header } from "./components/layout/Header";
import { ActionBar } from "./components/layout/ActionBar";
import { ManagerView } from "./components/manager/ManagerView";
import { ConflictDialog } from "./components/manager/ConflictDialog";
import { ApplySuiteConfirmDialog } from "./components/suites/ApplySuiteConfirmDialog";
import { ConfigPage } from "./components/config/ConfigPage";
import { ResourcesPage } from "./components/resources/ResourcesPage";
import { StatisticsPage } from "./components/statistics/StatisticsPage";
import { Alert, AlertDescription } from "./components/ui/alert";
import { Toaster } from "./components/ui/sonner";
import { TooltipProvider } from "./components/ui/tooltip";
import { resolveAppRoute } from "./navigation";

function navigate(route: Route) {
  const nextHash = route === "manager" ? "" : `#/${route}`;
  if (route === "suites" && window.location.hash === nextHash) {
    void enterSuiteManagerScope();
    return;
  }
  window.location.hash = nextHash;
}

async function notifyUsageTracingFailure(failure: UsageTracingHealthFailure): Promise<void> {
  try {
    const granted =
      (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (granted) {
      sendNotification({ title: "Local usage tracing is paused", body: failure.message });
    }
  } catch {
    // The in-app prompt remains available if the OS or its notification permission fails.
  }
}

export function App() {
  const status = useManagerStore((s) => s.status);
  const error = useManagerStore((s) => s.error);
  const data = useManagerStore((s) => s.data);
  const scope = useManagerStore((s) => s.scope);
  const pending = useManagerStore((s) => s.pendingKeys.length);
  const refresh = useManagerStore((s) => s.refresh);
  const setProgress = useManagerStore((s) => s.setProgress);

  const updatePhase = useUpdateStore((s) => s.phase);
  const availableUpdate = useUpdateStore((s) => s.available);

  const [route, setRoute] = useState<Route>(() => resolveAppRoute(window.location.hash).route);

  // Background update scan: throttled in the store to once per weekly window.
  // Evaluated on launch, on each app re-open (Dock click), and on a long-session
  // interval. The "Check for Updates…" menu item forces a non-silent check.
  useEffect(() => {
    void useUpdateStore.getState().maybeCheck();
    const reopen = onAppReopened(() => void useUpdateStore.getState().maybeCheck());
    const menu = onMenuCheckUpdates(() => void useUpdateStore.getState().check());
    const interval = window.setInterval(
      () => void useUpdateStore.getState().maybeCheck(),
      UPDATE_CHECK_INTERVAL_MS,
    );
    return () => {
      void reopen.then((fn) => fn());
      void menu.then((fn) => fn());
      window.clearInterval(interval);
    };
  }, []);

  // Prompt to install when a newer signed release is found. The toast persists
  // until acted on; dismissing it clears the pending update for this session.
  useEffect(() => {
    if (updatePhase !== "available" || !availableUpdate) return;
    toast("Update available", {
      id: "app-update",
      description: `Agentic Hub ${availableUpdate.version} is ready to install.`,
      duration: Infinity,
      action: {
        label: "Install & Relaunch",
        onClick: () => void useUpdateStore.getState().install(),
      },
      onDismiss: () => useUpdateStore.getState().dismiss(),
    });
  }, [updatePhase, availableUpdate]);

  useEffect(() => {
    const unlisten = onUsageTracingHealthFailed((failure) => {
      void notifyUsageTracingFailure(failure);
      toast.error("Local usage tracing is paused", {
        id: "usage-tracing-health",
        description: failure.message,
        duration: Infinity,
        action: { label: "Restart app", onClick: () => void restartApp() },
      });
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  // Global scope owns the manager refresh (scan → inspect). Runs on mount and
  // whenever the user returns to global scope, restoring the editable matrix.
  // Workspace scope loads its own read-only inventory via WorkspaceView.
  useEffect(() => {
    if (scope === "global") void refresh();
  }, [scope, refresh]);

  useEffect(() => {
    const onHash = () => {
      const resolved = resolveAppRoute(window.location.hash);
      setRoute(resolved.route);
      if (resolved.managerScope) void enterSuiteManagerScope();
    };
    onHash();
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

  // Palette locate: route to Manager in the requested scope and flag the row so
  // the Matrix surfaces it. Workspace locates activate the owning workspace
  // (loading its inventory) and namespace the item id to match its matrix rows;
  // global locates use the raw id against the editable matrix.
  useEffect(() => {
    const unlisten = onHubLocate((req) => {
      void (async () => {
        const manager = useManagerStore.getState();
        navigate("manager");
        if (req.scope === "global") {
          manager.setScope("global");
          useManagerFiltersStore.getState().setLocate(req.itemId);
          return;
        }
        manager.setScope("workspace");
        await useWorkspaceStore.getState().activate(req.workspaceId);
        useManagerFiltersStore.getState().setLocate(WORKSPACE_ID_PREFIX + req.itemId);
      })();
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  // Palette watching toggle: the new state is already persisted by the palette
  // window; just sync the header flag.
  useEffect(() => {
    const unlisten = onHubWatcherChanged((enabled) =>
      useManagerStore.getState().setWatching(enabled),
    );
    return () => void unlisten.then((fn) => fn());
  }, []);

  // Live-refresh on watcher / resync events. In global scope, rescan the
  // editable matrix (only when idle, so an incoming event never discards an
  // in-progress selection). In workspace scope, the inventory merges
  // globally-applied resources, so a shared-source change must reload it too.
  useEffect(() => {
    const unlisten = onSourcesChanged(() => {
      const s = useManagerStore.getState();
      if (s.scope === "global" || s.scope === "suite") {
        if (s.pendingKeys.length === 0) void refresh();
        return;
      }
      const id = useWorkspaceStore.getState().activeId;
      if (id) void s.loadWorkspace(id);
    });
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  // Live-refresh the workspace inventory when its tool dirs change. Only acts in
  // workspace scope with an active target selected.
  useEffect(() => {
    const unlisten = onWorkspaceChanged(() => {
      const s = useManagerStore.getState();
      if (s.scope !== "workspace") return;
      const id = useWorkspaceStore.getState().activeId;
      if (id) void s.loadWorkspace(id);
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

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
          {data && route === "statistics" && <StatisticsPage />}
          {route === "skills" && <ResourcesPage />}
          {route === "manager" && <ManagerView />}
        </main>
        {route === "manager" && scope === "global" && pending > 0 && <ActionBar />}
        <ConflictDialog />
        <ApplySuiteConfirmDialog />
      </div>
      <Toaster />
    </TooltipProvider>
  );
}
