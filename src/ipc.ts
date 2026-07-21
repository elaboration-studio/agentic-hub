// Typed IPC wrappers — one flat module over the Rust `cmd_*` handlers.
// Payloads are single typed objects; argument keys match the Rust parameter
// names. See docs/tech/modules/tauri-ipc-contract.md.

import { Channel, invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { check as checkUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApplyProgressEvent,
  ApplyResult,
  ApplySuiteResult,
  CapabilityItem,
  CliTool,
  CliToolStatus,
  ColorScheme,
  InspectResult,
  InstallContext,
  InstallScope,
  InstalledToolInventory,
  PlannedOperation,
  ScaffoldMode,
  ScaffoldResult,
  ScanResult,
  SessionMessage,
  SessionSummary,
  Settings,
  SkillCliStatus,
  SkillFavorite,
  SkillFavoritesState,
  SkillInstallEvent,
  SkillSearchHit,
  SourceConfig,
  SuiteCapabilityRef,
  SuiteDefinition,
  SuiteOwnership,
  SuiteStoreChangedEvent,
  SyncHooksResult,
  SyncRulesResult,
  ToolId,
  ToolsSettings,
  UsageStats,
  UsageTracerHooksSyncResult,
  UsageTracingHealthFailure,
  UsageTracingStatus,
  UsageDashboard,
  UsageDateRange,
  WorkspaceInventory,
  WorkspaceTarget,
  WorkspaceTargetsState,
} from "./types";

export type DesiredMap = Record<string, boolean>;

export const loadSettings = (): Promise<Settings> => invoke("cmd_load_settings");

/// The resolved source forest (stable `id`s filled in). Used by the install
/// window's Library-scope source picker.
export const resolveSources = (): Promise<SourceConfig[]> => invoke("cmd_resolve_sources");

export const setColorScheme = (colorScheme: ColorScheme): Promise<void> =>
  invoke("cmd_set_color_scheme", { colorScheme });

export const onColorSchemeChanged = (
  cb: (colorScheme: ColorScheme) => void,
): Promise<UnlistenFn> =>
  listen<ColorScheme>("color-scheme-changed", (event) => cb(event.payload));

export const saveSettings = (settings: Settings): Promise<void> =>
  invoke("cmd_save_settings", { settings });

export const setWatcherEnabled = (enabled: boolean): Promise<void> =>
  invoke("cmd_set_watcher_enabled", { enabled });

// ---- Command palette + cross-window navigation ----------------------------

export type NavRoute = "manager" | "suites" | "config";

/// Show / hide the floating command-palette window (also bound to the View menu
/// and the global shortcut).
export const togglePalette = (): Promise<void> => invoke("cmd_toggle_palette");

/// Show + focus the main window and hide the palette. Used by palette nav.
export const showMain = (): Promise<void> => invoke("cmd_show_main");

/// Ask the main window to navigate to a route (emitted from the palette).
export const emitHubNavigate = (route: NavRoute): Promise<void> =>
  emit("hub-navigate", route);

/// Main window: react to a palette navigation request.
export const onHubNavigate = (cb: (route: NavRoute) => void): Promise<UnlistenFn> =>
  listen<NavRoute>("hub-navigate", (e) => cb(e.payload));

/// A request to locate one item in the Hub's matrix. The main window routes to
/// Manager, switches to the requested scope (activating the owning workspace
/// for workspace locates), and highlights the matching matrix row.
export type LocateRequest =
  | { scope: "workspace"; workspaceId: string; itemId: string }
  | { scope: "global"; itemId: string };

/// Ask the main window to locate a workspace item (emitted from the palette).
export const emitHubLocate = (payload: LocateRequest): Promise<void> =>
  emit("hub-locate", payload);

/// Main window: react to a palette locate request.
export const onHubLocate = (cb: (payload: LocateRequest) => void): Promise<UnlistenFn> =>
  listen<LocateRequest>("hub-locate", (e) => cb(e.payload));

/// Palette: the watching toggle persisted a new watcher state (payload: the new
/// enabled flag). The main window syncs its header toggle without a re-fetch.
export const emitHubWatcherChanged = (enabled: boolean): Promise<void> =>
  emit("hub-watcher-changed", enabled);

/// Main window: react to a palette watcher toggle.
export const onHubWatcherChanged = (cb: (enabled: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("hub-watcher-changed", (e) => cb(e.payload));

/// Main window: the "Settings…" menu item (Cmd+,) was activated.
export const onMenuOpenConfig = (cb: () => void): Promise<UnlistenFn> =>
  listen("menu-open-config", () => cb());

export const rescanResync = (): Promise<void> => invoke("cmd_rescan_resync");

/// Fired by the watcher (and the resync fallback) after projections change.
export const onSourcesChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("sources-changed", () => cb());

/// Emit `sources-changed` from the frontend (the palette's inline toggle) so the
/// main window refreshes its matrix without waiting on the file watcher — which
/// may be paused. The main window only refreshes when it has no pending edits.
export const emitSourcesChanged = (): Promise<void> => emit("sources-changed");

export const scan = (sources: SourceConfig[]): Promise<ScanResult> =>
  invoke("cmd_scan", { input: { sources } });

export const scanInstalledTools = (tools: ToolsSettings): Promise<InstalledToolInventory> =>
  invoke("cmd_scan_installed_tools", { input: { tools } });

/// Materialize the bundled demo tree into the first source root. First-run
/// "empty start" affordance; re-scan after it resolves.
export const scaffoldDemo = (mode: ScaffoldMode): Promise<ScaffoldResult> =>
  invoke("cmd_scaffold_demo", { input: { mode } });

/// Open a file with the user's preferred editor (or system default when
/// `openWith` is absent). Validated server-side against known roots.
export const openPath = (path: string, openWith?: string): Promise<void> =>
  invoke("cmd_open_path", { input: { path, openWith: openWith ?? null } });

/// Reveal a file in the system file explorer (Finder / Explorer).
export const revealPath = (path: string): Promise<void> =>
  invoke("cmd_reveal_path", { input: { path } });

/// Read a capability file's text body (validated server-side against known
/// roots). Used by the palette to copy a command prompt to the clipboard.
export const readCapabilityBody = (path: string): Promise<string> =>
  invoke("cmd_read_capability_body", { input: { path } });

/// Outcome of attempting to paste into the frontmost app after a palette copy.
export interface PasteOutcome {
  pasted: boolean;
  needsPermission: boolean;
}

/// Hide the palette and simulate Cmd+V into the frontmost app (macOS,
/// Accessibility-gated). No-op on other platforms.
export const pasteToFrontmost = (): Promise<PasteOutcome> =>
  invoke("cmd_paste_to_frontmost");

/// Write text to the system clipboard via the clipboard-manager plugin.
export const copyText = (text: string): Promise<void> =>
  writeText(text);

/// Open an external http(s) URL in the default browser. Anchor navigation is a
/// no-op inside the Tauri WebView, so external links route through Rust.
export const openUrl = (url: string): Promise<void> =>
  invoke("cmd_open_url", { input: { url } });

export const inspect = (
  items: CapabilityItem[],
  tools: ToolsSettings,
): Promise<InspectResult> => invoke("cmd_inspect", { items, tools });

export const addSource = (label: string, path: string): Promise<Settings> =>
  invoke("cmd_add_source", { input: { label, path } });

export const removeSource = (id: string): Promise<Settings> =>
  invoke("cmd_remove_source", { input: { id } });

export const plan = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
  force = false,
): Promise<PlannedOperation[]> =>
  invoke("cmd_plan", { input: { toolId, items, desiredEnabledByItemId, force } });

export const apply = (operations: PlannedOperation[]): Promise<ApplyResult> =>
  invoke("cmd_apply", { operations });

export const syncRules = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
): Promise<SyncRulesResult> =>
  invoke("cmd_sync_rules", { input: { toolId, items, desiredEnabledByItemId } });

export const syncHooks = (
  toolId: ToolId,
  items: CapabilityItem[],
  desiredEnabledByItemId: DesiredMap,
): Promise<SyncHooksResult> =>
  invoke("cmd_sync_hooks", { input: { toolId, items, desiredEnabledByItemId } });

// ---- Local usage tracing --------------------------------------------------

export const usageTracingStatus = (): Promise<UsageTracingStatus> =>
  invoke("cmd_usage_tracing_status");

export const setUsageTracingEnabled = (enabled: boolean): Promise<UsageTracingStatus> =>
  invoke("cmd_set_usage_tracing_enabled", { enabled });

export const syncUsageTracerHooks = (): Promise<UsageTracerHooksSyncResult> =>
  invoke("cmd_sync_usage_tracer_hooks");

export const onUsageTracingHealthFailed = (
  cb: (failure: UsageTracingHealthFailure) => void,
): Promise<UnlistenFn> =>
  listen<UsageTracingHealthFailure>("usage-tracing-health-failed", (event) => cb(event.payload));

export const queryUsageStats = (items: CapabilityItem[]): Promise<UsageStats[]> =>
  invoke("cmd_query_usage_stats", { items });

export const queryUsageDashboard = (
  items: CapabilityItem[],
  range: UsageDateRange,
): Promise<UsageDashboard> => invoke("cmd_query_usage_dashboard", { items, range });

// ---- Session Explorer -------------------------------------------------------

export interface ListSessionsInput {
  tools?: ToolId[];
  workspace?: string;
  query?: string;
  range: UsageDateRange;
}

export const listSessions = (input: ListSessionsInput): Promise<SessionSummary[]> =>
  invoke("cmd_list_sessions", { input });

export const getSession = (sessionKey: string): Promise<SessionMessage[]> =>
  invoke("cmd_get_session", { input: { sessionKey } });

export const recordCommandPaletteUsage = (
  capabilityId: string,
  pasted: boolean,
): Promise<void> => invoke("cmd_record_command_palette_usage", { capabilityId, pasted });

export const onApplyProgress = (
  cb: (e: ApplyProgressEvent) => void,
): Promise<UnlistenFn> =>
  listen<ApplyProgressEvent>("apply-progress", (event) => cb(event.payload));

// ---- Suites ---------------------------------------------------------------

export interface SuiteCreatePayload {
  name: string;
  description?: string | null;
  // Source-qualified refs. The Rust core also tolerates bare-string entries
  // (legacy), upgrading them in place on the next write.
  capabilities: SuiteCapabilityRef[];
}

export interface SuiteUpdatePayload {
  name?: string;
  description?: string | null;
  capabilities?: SuiteCapabilityRef[];
  // Mark/unmark this suite as the single base suite (cleared on every other).
  isBase?: boolean;
}

export const listSuites = (): Promise<SuiteDefinition[]> => invoke("cmd_list_suites");

export const getSuite = (id: string): Promise<SuiteDefinition | null> =>
  invoke("cmd_get_suite", { id });

export const createSuite = (input: SuiteCreatePayload): Promise<SuiteDefinition> =>
  invoke("cmd_create_suite", { input });

export const updateSuite = (
  id: string,
  changes: SuiteUpdatePayload,
): Promise<SuiteDefinition> => invoke("cmd_update_suite", { input: { id, ...changes } });

export const deleteSuite = (id: string): Promise<void> =>
  invoke("cmd_delete_suite", { id });

export const applySuite = (
  toolId: ToolId,
  suiteId: string,
  preserveManual = false,
): Promise<ApplySuiteResult> =>
  invoke("cmd_apply_suite", { input: { toolId, suiteId, preserveManual } });

export const suiteApplyPreview = (
  toolId: ToolId,
  suiteId: string,
): Promise<string[]> =>
  invoke("cmd_suite_apply_preview", { input: { toolId, suiteId } });

// Mark the single base suite (its capabilities union into every applied suite),
// or clear it with `null`. Re-applies every bound tool.
export const setBaseSuite = (id: string | null): Promise<void> =>
  invoke("cmd_set_base_suite", { id });

// Which suite owns each (tool, item) projection — drives the Manager cell lock.
export const suiteOwnership = (): Promise<SuiteOwnership[]> =>
  invoke("cmd_suite_ownership");

export const onSuiteStoreChanged = (
  cb: (e: SuiteStoreChangedEvent) => void,
): Promise<UnlistenFn> =>
  listen<SuiteStoreChangedEvent>("suite-store-changed", (event) => cb(event.payload));

// ---- Workspace scope ------------------------------------------------------

export const pickWorkspaceDir = (): Promise<WorkspaceTarget> =>
  invoke("cmd_pick_workspace_dir");

export const listWorkspaceTargets = (): Promise<WorkspaceTargetsState> =>
  invoke("cmd_list_workspace_targets");

export const removeWorkspaceTarget = (id: string): Promise<void> =>
  invoke("cmd_remove_workspace_target", { id });

export const setActiveWorkspaceTarget = (id: string): Promise<void> =>
  invoke("cmd_set_active_workspace_target", { id });

/// Read-only inventory of one workspace's installed agentic resources.
export const scanWorkspace = (workspaceId: string): Promise<WorkspaceInventory> =>
  invoke("cmd_scan_workspace", { workspaceId });

/// Fired by the watcher after a workspace's tool dirs change.
export const onWorkspaceChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("workspace-changed", () => cb());

// ---- CLI tool preflight ---------------------------------------------------

/// The CLI tool catalog (bundled set merged with any user-local override).
export const listToolCatalog = (): Promise<CliTool[]> =>
  invoke("cmd_list_tool_catalog");

/// Probe one tool by id: installed + version, and auth state when applicable.
export const checkTool = (id: string): Promise<CliToolStatus> =>
  invoke("cmd_check_tool", { input: { id } });

// ---- Skill sources (skills.sh) --------------------------------------------

/// Search a provider's keyless public index. Runs through Rust (`cmd_search_skills`)
/// rather than a WebView fetch: the skills.sh search endpoint sends no CORS
/// header, so the WebView can't call it directly. No API key is involved.
export const searchSkills = (
  provider: string,
  query: string,
  limit = 30,
): Promise<SkillSearchHit[]> =>
  invoke("cmd_search_skills", { input: { provider, query, limit } });

export const skillCliCheck = (provider: string): Promise<SkillCliStatus> =>
  invoke("cmd_skill_cli_check", { input: { provider } });

export const listSkillFavorites = (): Promise<SkillFavoritesState> =>
  invoke("cmd_list_skill_favorites");

export const addSkillFavorite = (favorite: SkillFavorite): Promise<SkillFavorite> =>
  invoke("cmd_add_skill_favorite", { favorite });

export const removeSkillFavorite = (provider: string, id: string): Promise<void> =>
  invoke("cmd_remove_skill_favorite", { input: { provider, id } });

/// Fired by the watcher when the favorites file changes on disk (e.g. a
/// `git pull` on a synced custom path), so the starred list reloads live.
export const onSkillsFavoritesChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("skills-favorites-changed", () => cb());

// ---- Skill install window (dedicated, live-streaming) ---------------------

/// Open (or focus) the install window targeting a workspace. The window reads
/// its context on mount via `takeInstallContext`.
export const openInstallWindow = (workspaceId: string): Promise<void> =>
  invoke("cmd_open_install_window", { workspaceId });

/// Install window: read the workspace context set when it was opened.
export const takeInstallContext = (): Promise<InstallContext> =>
  invoke("cmd_take_install_context");

export interface InstallSkillStreamPayload {
  provider: string;
  installRef: string;
  scope: InstallScope;
  /// Workspace scope only.
  workspaceId?: string;
  /// Library scope only: which source root to install into.
  sourceId?: string;
  /// Library scope only: destination subpath under `skills/` (empty lands the
  /// skill directly at `skills/<name>/`).
  destSubpath?: string;
  /// The one skill slug to install — pins `--skill` so a multi-skill repo never
  /// opens an interactive picker. Required for Library scope.
  slug: string;
  /// Workspace scope only.
  toolIds: ToolId[];
}

/// Install one skill, streaming stdout/stderr lines (then a terminal `done`)
/// over `onEvent`. Resolves when the run finishes (or is cancelled).
export const installSkillStream = (
  input: InstallSkillStreamPayload,
  onEvent: Channel<SkillInstallEvent>,
): Promise<void> => invoke("cmd_install_skill_stream", { input, onEvent });

/// Open (or focus) the install window in update mode for one skills.sh-managed
/// skill. The window reads the `update` context on mount and runs `skills update`.
export const openUpdateWindow = (
  workspaceId: string,
  provider: string,
  installRef: string,
  name: string,
): Promise<void> =>
  invoke("cmd_open_update_window", { workspaceId, provider, installRef, name });

/// Open (or focus) the install window in update mode for one Library-scope
/// (source-root-locked) skill — the Global Manager's "Update via skills.sh"
/// row action. No workspace is involved.
export const openLibraryUpdateWindow = (
  provider: string,
  installRef: string,
  name: string,
  sourceId: string,
  destSubpath: string,
): Promise<void> =>
  invoke("cmd_open_library_update_window", {
    provider,
    installRef,
    name,
    sourceId,
    destSubpath,
  });

export interface UpdateSkillStreamPayload {
  provider: string;
  scope: InstallScope;
  /// Workspace scope only.
  workspaceId?: string;
  /// The skill's install name — the lock key passed to update.
  name: string;
  /// Library scope only: which source root to re-install into.
  sourceId?: string;
  /// Library scope only: the ref to re-install (recorded at install time).
  installRef?: string;
  /// Library scope only: `--skill` slug, when it differs from `name`.
  slug?: string;
  /// Library scope only: destination subpath under `skills/`, recorded at
  /// install time.
  destSubpath?: string;
}

/// Update one already-installed skill, streaming output (then a terminal `done`)
/// over `onEvent`. Shares the install window's streaming + cancel machinery.
export const updateSkillStream = (
  input: UpdateSkillStreamPayload,
  onEvent: Channel<SkillInstallEvent>,
): Promise<void> => invoke("cmd_update_skill_stream", { input, onEvent });

/// Kill the in-flight install (if any). The current stream then ends as
/// `cancelled`.
export const cancelInstall = (): Promise<void> => invoke("cmd_cancel_install");

/// Install window: the targeted workspace context changed (window reopened for a
/// different workspace) — re-read it.
export const onInstallContextChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("install-context-changed", () => cb());

// ---- App self-update (Tauri updater + R2 feed) ----------------------------

/// One available app update: the version + notes to surface, plus the opaque
/// plugin handle used to download and install it. Resolved by `checkForUpdate`.
export interface AvailableUpdate {
  version: string;
  notes: string | null;
  /// Plugin-side update handle. Pass it to `installUpdate`; not for direct use.
  handle: Update;
}

/// Check the R2-hosted feed for a newer minisign-signed release. Resolves the
/// available update (version + notes + install handle), or null when current.
export const checkForUpdate = async (): Promise<AvailableUpdate | null> => {
  const update = await checkUpdate();
  if (!update) return null;
  return { version: update.version, notes: update.body ?? null, handle: update };
};

/// Download + install an available update (the plugin verifies its signature),
/// then relaunch into the new version. Does not return on success.
export const installUpdate = async (update: AvailableUpdate): Promise<void> => {
  await update.handle.downloadAndInstall();
  await relaunch();
};

/// Restart the current app after an explicit user action.
export const restartApp = async (): Promise<void> => relaunch();

/// Fired by Rust when the app is re-opened (Dock click) so the frontend can run
/// a throttled background update check.
export const onAppReopened = (cb: () => void): Promise<UnlistenFn> =>
  listen("app-reopened", () => cb());

/// Fired by the "Check for Updates…" menu item — an explicit, non-silent check.
export const onMenuCheckUpdates = (cb: () => void): Promise<UnlistenFn> =>
  listen("menu-check-updates", () => cb());
