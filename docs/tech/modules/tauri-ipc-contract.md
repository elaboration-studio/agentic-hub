# Module: Tauri IPC Contract

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.permissions.md](../../../ARCHITECTURE.permissions.md)
Related Docs: [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/modules/suite-presets.md](./suite-presets.md), [docs/tech/modules/workspace-inventory.md](./workspace-inventory.md)

## Purpose

Define the complete IPC surface between the React WebView and the Rust core. Every IPC call is a typed `invoke(cmd_name, payload)` plus optional event channels. Types are mirrored from Rust to TypeScript via `ts-rs` codegen.

## Conventions

- **Naming.** All command names use `cmd_` prefix, snake_case (e.g. `cmd_scan`, `cmd_scan_workspace`)
- **Payloads.** All command inputs are single typed objects, never positional args
- **Outputs.** All command outputs are typed objects or `()` (returns `null` to JS)
- **Errors.** All commands return `Result<T, IpcError>` in Rust, which maps to a thrown error in JS with a typed `code` and `message`
- **Events.** Use Tauri's event system for streaming / cross-window notifications. Events are named with `kebab-case`
- **Async.** All commands are `async`. Long-running ops emit progress events

## Error envelope

```rust
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct IpcError {
    pub code: String,        // stable machine-readable code
    pub message: String,     // human-readable, suitable for surfacing in UI
    pub details: Option<serde_json::Value>,  // structured detail for specific errors
}
```

Common error codes:

| Code | Meaning |
|------|---------|
| `path_not_found` | A required path does not exist |
| `path_invalid` | A path failed canonicalization or validation |
| `permission_denied` | OS-level permission error |
| `out_of_workspace` | A workspace target resolved outside the workspace root |
| `tool_unavailable` | A tool's configured paths are missing or unreadable |
| `tool_disabled` | A tool is disabled in settings |
| `tool_unsupported_in_scope` | E.g. OpenClaw in workspace scope |
| `suite_not_found` | A suite id does not exist in the store |
| `suite_name_collision` | A suite name is already in use |
| `workspace_not_found` | A workspace target id does not exist in the store |
| `rule_sync_malformed_markers` | Instruction file has malformed managed-block markers |
| `conflict_real_file_at_target` | A real file blocks a write |
| `hook_manifest_invalid` | A `hook.json` failed schema validation |
| `hook_target_broken_json` | A tool's hook config file is malformed and cannot be safely rewritten |
| `source_path_invalid` | A configured source path failed canonicalization or is not a directory |
| `path_not_openable` | A path passed to `cmd_open_path` / `cmd_reveal_path` resolved outside every known root |
| `open_failed` | The opener plugin could not open the path |
| `reveal_failed` | The opener plugin could not reveal the path |
| `invalid_shortcut` | The palette accelerator string in settings is malformed |
| `shortcut_register_failed` | The global palette shortcut could not be registered with the OS |
| `internal` | Catch-all unexpected error; surface for bug reports |

## Settings commands

### `cmd_load_settings() -> Settings`

Reads `~/.agentic-hub/config.json` and returns the parsed settings. If the file does not exist, returns defaults. If parsing fails, returns `Err(IpcError { code: "internal", ... })` with the parse error; the caller may offer to overwrite with defaults.

### `cmd_save_settings(settings: Settings) -> ()`

Validates and atomically writes settings. Validation includes: non-empty `shared_root`; per-tool paths well-formed; `paletteShortcut` passes `is_valid_shortcut` (`invalid_shortcut` otherwise). Re-subscribes the source watcher to the current roots when it is running, then re-registers the global palette accelerator (`shortcut_register_failed` if the OS rejects it).

### `cmd_set_watcher_enabled(input: { enabled: boolean }) -> ()`

Persists `settings.watcherEnabled` and starts or stops the source watcher immediately. See [watcher.md](./watcher.md).

### `cmd_toggle_palette() -> ()`

Show (and focus) or hide the floating command-palette window. Bound to the global accelerator (handled in Rust) and the View > Command Palette menu item; also callable from the UI.

### `cmd_show_main() -> ()`

Show + focus the main window and hide the palette. Used by palette navigation commands that route back into the main window (paired with the `hub-navigate` event).

### `cmd_rescan_resync() -> ()`

Recovery fallback: a full rescan + resync of every enabled tool (no newcomer auto-enable) plus the active workspace, then emits `sources-changed`. Use when the watcher is paused or projections look out of sync.

```typescript
type Settings = {
  sources: SourceConfig[];        // ordered by priority
  sharedRoot: string;             // deprecated; one-release fallback when sources is empty
  suitesPath: string | null;      // custom suite-store path, or null for the default
  watcherEnabled: boolean;
  editor: EditorPref;             // preferred editor for "open original"
  paletteShortcut: string;        // global accelerator, e.g. "Cmd+Alt+A"
  tools: ToolsSettings;
};

type EditorPref = {
  kind: string;                   // 'default' | 'vscode' | 'cursor' | 'custom'
  customApp: string | null;       // app name/path when kind == 'custom'
};

type SourceConfig = {
  id: string;                     // stable slug derived from label
  label: string;
  path: string;                   // absolute, normalized
};

type ToolsSettings = {
  codex: ToolSettings;
  claude: ToolSettings;
  cursor: ToolSettings;
  openclaw: ToolSettings;
};

type ToolSettings = {
  enabled: boolean;
  skillsPath: string;
  agentsPath: string;
  rulesPath: string;
  instructionsPath: string | null;
  hooksEnabled: boolean;          // default true for codex/claude/cursor, false for openclaw
  hooksFile: string | null;       // null for openclaw (no hook support)
};
```

Hook settings defaults: `~/.codex/hooks.json`, `~/.claude/settings.json`, `~/.cursor/hooks.json`. OpenClaw has `hooksEnabled: false` and `hooksFile: null`.

## Scan & inspect commands

### `cmd_scan(input: { sources: SourceConfig[] }) -> ScanResult`

Walks every source in priority order, dedupes by `${kind}:${relativePath}` (first source wins), and reports shadowed duplicates and missing source folders as `errors`. The legacy single-root `cmd_scan(shared_root)` is retained as a thin wrapper.

```typescript
type ScanResult = {
  items: CapabilityItem[];
  errors: ScanError[];
};

type CapabilityItem = {
  id: string;                  // e.g. "skill:dev/repo-research" (source-free)
  kind: 'skill' | 'agent' | 'rule' | 'hook';
  name: string;
  sourcePath: string;          // absolute (for hooks: the hook folder = ${HOOK_DIR})
  relativePath: string;        // relative to <source>/<kind>/
  sourceId: string;            // which source contributed this item
  sourceLabel: string;
  source: SourceRef;           // portable cross-device source identity
  valid: boolean;
  validationErrors: string[];
};

type SourceRef = {
  relHome: string;             // home-relative path (~/.agentic) or absolute if outside ~
  folder: string;              // last path component (.agentic)
};

type ScanError = {
  path: string;
  message: string;             // e.g. "shadowed by higher-priority source 'Arno'"
};
```

### `cmd_add_source(input: { label: string, path: string }) -> Settings`

The shell opens a Tauri folder dialog before this call. Validates uniqueness of label and normalized path, appends to `settings.sources`, persists, and emits `settings-changed`. Errors: `source_path_invalid` if the path fails canonicalization or is not a directory.

### `cmd_remove_source(input: { id: string }) -> Settings`

Splices the matching entry out of `settings.sources` and persists. Never touches files on disk. Removing the only source falls back to the legacy `sharedRoot` Default.

### `cmd_inspect(items: CapabilityItem[], tools: ToolsSettings) -> InspectResult`

```typescript
type InspectResult = {
  states: ToolCapabilityState[];
  adapterStatuses: AdapterStatus[];
};

type ToolCapabilityState = {
  tool: 'codex' | 'claude' | 'cursor' | 'openclaw';
  itemId: string;
  targetPath: string;
  state: 'enabled' | 'disabled' | 'broken' | 'stale' | 'foreign_file' | 'foreign_link';
  currentLinkTarget?: string;
};

type AdapterStatus = {
  tool: ToolId;
  available: boolean;
  unavailableReason?: string;  // e.g. "Tool path missing: ~/.cursor/agents"
};
```

## Plan & apply commands

### `cmd_plan(input: PlanInput) -> PlannedOperation[]`

```typescript
type PlanInput = {
  toolId: ToolId;
  itemIds: string[];             // all currently-scanned item ids
  desiredEnabledByItemId: Record<string, boolean>;
  force?: boolean;               // default false; confirmed foreign_file take-over
};

type PlannedOperation = {
  tool: ToolId;
  itemId: string;
  targetPath: string;
  sourcePath?: string;
  kind: 'create_link' | 'remove_link' | 'replace_link'
      | 'create_managed_copy' | 'remove_managed_copy' | 'replace_managed_copy'
      | 'sync_json_section' | 'clear_json_section'
      | 'skip_conflict';
  reason: string;
  force: boolean;                // applier may delete a real file/dir at the target
};
```

The plan is computed against fresh disk state. The caller does not pass current state; the planner re-inspects internally.

`force` is the confirmed destructive resolution of a `foreign_file` conflict (a real, user-owned file/dir blocking a tool target). When `false` (default), `(foreign_file, enable)` yields a `skip_conflict` and nothing is written — real files are never overwritten silently. When `true`, the planner emits a take-over (`replace_link` / `replace_managed_copy` with `force: true`), and the applier deletes the blocking file/dir before projecting. The UI sets `force: true` only after the user confirms the Apply-time warning dialog; the watcher and suite/workspace applies always pass `false`.

### `cmd_apply(operations: PlannedOperation[]) -> ApplyResult`

```typescript
type ApplyResult = {
  created: number;
  removed: number;
  replaced: number;
  refreshed: number;
  skipped: number;
  errors: ApplyError[];
};

type ApplyError = {
  operation: PlannedOperation;
  message: string;
  code: string;
};
```

Emits `apply-progress` events per operation (see Events below).

`sync_json_section` / `clear_json_section` operations (hooks) are grouped by target file and executed as a single read–merge–write per file inside the apply, so foreign entries are partitioned and preserved exactly once. Before planning, the caller passes only hooks whose effective targets include the focused tool; the planner's `filter_desired_enabled_for_tool` step drops the rest with a note. See [hook-projection-sync.md](./hook-projection-sync.md).

### `cmd_sync_rules(input: SyncRulesInput) -> SyncRulesResult`

```typescript
type SyncRulesInput = {
  toolId: ToolId;
  items: CapabilityItem[];
  states: ToolCapabilityState[];
};

type SyncRulesResult = {
  outcome: 'wrote' | 'removed' | 'no_op';
  errors: RuleSyncError[];
};
```

## Suite commands

### `cmd_list_suites() -> SuiteDefinition[]`

```typescript
type SuiteDefinition = {
  id: string;                          // UUID
  name: string;
  description?: string;
  capabilities: SuiteCapabilityRef[];  // source-qualified refs
  isBase: boolean;                     // single base suite; merged into every apply (legacy = false)
  createdAt: string;                   // ISO 8601
  updatedAt: string;
};

type SuiteCapabilityRef = {
  cap: string;                  // bare capability id (skill:dev/tdd)
  source: SourceRef | null;     // portable source identity; null = legacy/unqualified
};
```

`SuiteCapabilityRef` deserializes tolerantly from a legacy bare string
(`"skill:dev/tdd"`), so existing suite files load unchanged and upgrade to the
object form on the next write. See [suite-presets.md](./suite-presets.md).

### `cmd_get_suite(id: string) -> SuiteDefinition | null`

### `cmd_create_suite(input: SuiteCreateInput) -> SuiteDefinition`

```typescript
type SuiteCreateInput = {
  name: string;
  description?: string;
  capabilities: SuiteCapabilityRef[];  // bare strings also accepted (legacy)
};
```

Errors: `suite_name_collision` if `name` is already in use.

### `cmd_update_suite(id: string, input: SuiteUpdateInput) -> SuiteDefinition`

```typescript
type SuiteUpdateInput = {
  name?: string;
  description?: string;
  capabilities?: SuiteCapabilityRef[];
  isBase?: boolean;   // mark/unmark base; true clears the flag on every other suite
};
```

Side effect: after the update, the affected bound tools are re-applied as a
full reset (base-merged) so projections track the new set, serialized against
the watcher via the reconcile guard. A **normal** suite re-syncs only its bound
tools; the **base** suite re-syncs **every** binding. Emits `sources-changed`
when any tool was re-applied. Also opportunistically qualifies unqualified refs
against the live scan and persists the upgrade (source backfill).

### `cmd_delete_suite(id: string) -> ()`

Side effect: drops every suite<->tool binding referencing this suite. The
tools' on-disk projections are left untouched (delete is not a tool wipe).

### `cmd_apply_suite(input: ApplySuiteInput) -> ApplySuiteResult`

```typescript
type ApplySuiteInput = {
  toolId: ToolId;
  suiteId: string;
};

type ApplySuiteResult = {
  applyResult: ApplyResult;
  skippedStale: number;          // present-source/unqualified refs with no match
  skippedAbsentSource: number;   // qualified refs whose source isn't on this machine (preserved)
  suite: { id: string; name: string };
};
```

Side effect: records a suite<->tool binding (`record(toolId, suiteId)`,
upsert per tool) so a later `cmd_update_suite` re-syncs this tool, and
backfills unqualified refs against the live scan. Both the palette suite-apply
flow and the Suites page flow through here. The base suite's capabilities are
unioned into the effective set before apply (`merge_base_caps`); the recorded
binding is always the **selected** suite, not the base. Refs qualified to a
source absent on this machine are skipped and preserved — never deleted, never
mis-resolved onto a same-named local capability.

### `cmd_set_base_suite(id: string | null) -> ()`

Marks `id` as the single base suite (clearing the flag on every other), or
clears the base entirely with `null`. Errors `suite_not_found` on an unknown
id. Side effect: re-applies **every** bound tool (each base-merged) so all
projections pick up or drop the new base, then emits `sources-changed` and
`suite-store-changed` (kind `base-changed`).

### `cmd_suite_ownership() -> SuiteOwnership[]`

Resolves which suite manages each `(tool, item)` projection, for the Manager to
lock those cells and name the owner on hover. Computed from a fresh scan, the
live bindings, the suites, and the base suite.

```typescript
type SuiteOwnership = {
  tool: ToolId;
  itemId: string;
  suiteId: string;
  suiteName: string;
  fromBase: boolean;   // true when owned via the base merge, not the bound suite
};
```

Bound-suite ownership wins when an item is in both the bound suite and the base.

## Workspace commands

### `cmd_pick_workspace_dir() -> WorkspaceTarget`

Opens a Tauri folder dialog. Returns the picked workspace target after canonicalizing and adding to the LRU store.

```typescript
type WorkspaceTarget = {
  id: string;
  label: string;
  dir: string;
  lastUsedAt: string;
};
```

Cancelled dialog returns `Err(IpcError { code: "user_cancelled" })`.

### `cmd_list_workspace_targets() -> WorkspaceTargetsState`

```typescript
type WorkspaceTargetsState = {
  workspaceTargets: WorkspaceTarget[];
  workspaceActiveId: string | null;
};
```

`cmd_pick_workspace_dir`, `cmd_remove_workspace_target`, and `cmd_set_active_workspace_target` restart the filesystem watcher so it re-subscribes to the new active workspace's tool dirs.

### `cmd_remove_workspace_target(id: string) -> ()`

### `cmd_set_active_workspace_target(id: string) -> ()`

### `cmd_scan_workspace(workspaceId: string) -> WorkspaceInventory`

Read-only inventory of one workspace's installed agentic resources. Resolves the
target dir from the store, then walks each workspace tool's own directories
(`.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
`AGENTS.md`, `CLAUDE.md`). Performs no writes. Only present resources are
returned, so every state is `enabled`.

```typescript
type WorkspaceInventory = {
  items: CapabilityItem[];          // one row per distinct resource, sorted by id
  states: ToolCapabilityState[];    // one per (tool, present item), always 'enabled'
  errors: ScanError[];
};
```

Errors: `workspace_not_found` (unknown id).

## Scaffold command

### `cmd_scaffold_demo(input: ScaffoldDemoInput) -> ScaffoldResult`

```typescript
type ScaffoldDemoInput = {
  mode: 'merge' | 'overwrite';
};

type ScaffoldResult = {
  written: number;
  skipped: number;             // existing files preserved in merge mode
  replaced: number;            // existing files overwritten in overwrite mode
  errors: string[];            // per-file failures ("<relative path>: <reason>")
  destinationRoot: string;     // resolved source root the tree was written into
};
```

The destination is the first configured source root (the legacy `sharedRoot`
when no sources are set). The empty-state in the manager calls this with
`merge`; the bundled tree is embedded in the binary via `include_dir!`.

## Open / reveal commands

These let the UI open a capability's original file in the user's preferred
editor and reveal it in the system file explorer. The WebView never holds
opener or FS scope: the command validates the path server-side (must
canonicalize under a configured source root, a tool target path, or a known
workspace dir) and then calls `tauri-plugin-opener` from Rust.

### `cmd_open_path(input: { path: string, openWith: string | null }) -> ()`

Opens `path` with `openWith` (an app name/path) or the OS default when null.
Errors: `path_not_openable`, `open_failed`.

### `cmd_reveal_path(input: { path: string }) -> ()`

Reveals `path` in Finder / Explorer. Errors: `path_not_openable`, `reveal_failed`.

## Events

Events are emitted from Rust and listened to in JS via `listen('event-name', ...)`.

### `apply-progress`

Emitted during `cmd_apply` after each operation completes (success or failure).

```typescript
type ApplyProgressEvent = {
  applyId: string;             // correlates with the cmd_apply call
  operationIndex: number;      // 0-based
  totalOperations: number;
  operation: PlannedOperation;
  success: boolean;
  error?: ApplyError;
};
```

### `workspace-changed`

Emitted (no payload) after the watcher detects a change under the active
workspace's tool dirs (`.cursor`, `.claude`, `.agents`, `.codex`, `AGENTS.md`,
`CLAUDE.md`). The UI reloads the active workspace inventory while in workspace
scope. See [watcher.md](./watcher.md).

### `suite-store-changed`

Emitted globally when `suite_store` is mutated by any window. Lets all windows refresh their suite-aware UI.

```typescript
type SuiteStoreChangedEvent = {
  kind: 'created' | 'updated' | 'deleted' | 'base-changed';
  suiteId: string;
};
```

### `sources-changed`

Emitted (no payload) after the source watcher — or the `cmd_rescan_resync` fallback — reconciles projections following a source-root file change. The UI listens and re-scans + re-inspects, skipping the refresh while the user has unapplied edits. See [watcher.md](./watcher.md).

### `menu-open-config`

Emitted (no payload) when the native "Settings…" menu item (Cmd+,) is activated. The main window listens and routes to the Config route.

### `hub-navigate`

Emitted by the palette window (payload: a route string `'manager' | 'suites' | 'config'`) when a navigation command runs. The main window listens and switches route; the palette then hides via `cmd_show_main`.

### `hub-locate`

Emitted by the palette window when a workspace search result is chosen. The main window switches to Manager + Workspace scope, activates the owning workspace (loading its inventory), and flags the matching matrix row so the `Matrix` expands its folders, scrolls to it, and highlights it briefly. The palette then surfaces the main window via `cmd_show_main`.

```typescript
type LocateRequest = { workspaceId: string; itemId: string };  // raw (non-namespaced) item id
```

### `settings-changed`

Emitted globally when `cmd_save_settings` succeeds.

```typescript
type SettingsChangedEvent = {};  // empty; receivers re-fetch
```

### `workspace-targets-changed`

Emitted globally when the workspace target store is mutated.

```typescript
type WorkspaceTargetsChangedEvent = {};
```

## Concurrency model

- The Rust core uses a global `tokio::sync::Mutex<()>` named `apply_lock` to serialize:
  - `cmd_apply`
  - `cmd_apply_suite`
  - `cmd_sync_rules`
- Settings save and suite CRUD are also serialized via their own per-resource mutexes
- Scan and inspect are read-only and run concurrently

## Capability requirements

Every command listed here must appear in `src-tauri/capabilities/default.json` under the main window's permission list. The Suite Manager window inherits the same permission set in v1 to keep things simple; a future tighter capability split is possible.

See [ARCHITECTURE.permissions.md](../../../ARCHITECTURE.permissions.md) for the full capability model.

## Codegen workflow

```bash
cargo test -p agentic-core --features=ts-export
# emits TS type files into src/types/generated/
```

Run before each commit that touches Rust-side IPC types. The generated files are committed.

## Versioning

In v1, the IPC contract is unversioned — the app is shipped as a single binary so client and server are always the same version. If we ever ship the Rust core as a separate sidecar or expose it over a socket, we will add a `version` field to each command's payload and a handshake step.

## Open questions

- Should we add per-window typed wrappers in TS (`mainIpc.scan(...)`, `suiteIpc.list(...)`) or one flat module? Decision: one flat module per concern (`ipc/scan.ts`, `ipc/suites.ts`, etc.); keeps imports clear without over-namespacing.
- Should long-running ops accept an abort signal? Decision: defer. Apply ops are short enough that cancellation is not critical. Add later if needed.
- Should event payloads include timestamps for client-side ordering? Decision: defer; Tauri's local event channel preserves order.
