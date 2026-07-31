# Module: Tauri IPC Contract

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-17
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
| `suite_binding_not_found` | A requested tool has no live suite binding to re-sync |
| `suite_name_collision` | A suite name is already in use |
| `workspace_not_found` | A workspace target id does not exist in the store |
| `rule_sync_malformed_markers` | Instruction file has malformed managed-block markers |
| `conflict_real_file_at_target` | A real file blocks a write |
| `hook_manifest_invalid` | A `hook.json` failed schema validation |
| `hook_target_broken_json` | A tool's hook config file is malformed and cannot be safely rewritten |
| `source_path_invalid` | A configured source path failed canonicalization or is not a directory |
| `path_not_openable` | A path passed to `cmd_open_path` / `cmd_reveal_path` resolved outside every known root |
| `url_not_openable` | A URL passed to `cmd_open_url` was not an absolute `http`/`https` URL with a host |
| `open_failed` | The opener plugin could not open the path |
| `reveal_failed` | The opener plugin could not reveal the path |
| `session_source_unavailable` | A session's source file was missing or unreadable on a `cmd_get_session` detail read |
| `invalid_shortcut` | The palette accelerator string in settings is malformed |
| `shortcut_register_failed` | The global palette shortcut could not be registered with the OS |
| `invalid_skill_ref` | A skill install reference failed `owner/repo` validation |
| `unknown_provider` | A skill source provider id is not registered |
| `skill_search` | A skill source search request failed (network / non-200 / malformed) |
| `skill_cli_missing` | The provider CLI (`npx`/Node) was not found on the resolved `PATH` |
| `install_failed` | A skill install subprocess could not be run |
| `internal` | Catch-all unexpected error; surface for bug reports |

## Settings commands

### `cmd_load_settings() -> Settings`

Reads `~/.agentic-hub/config.json` and returns the parsed settings. If the file does not exist, returns defaults. If parsing fails, returns `Err(IpcError { code: "internal", ... })` with the parse error; the caller may offer to overwrite with defaults.

### `cmd_save_settings(settings: Settings) -> ()`

Validates and atomically writes settings. Validation includes: non-empty `shared_root`; per-tool paths well-formed; `paletteShortcut` passes `is_valid_shortcut` (`invalid_shortcut` otherwise). Re-subscribes the source watcher to the current roots when it is running, then re-registers the global palette accelerator (`shortcut_register_failed` if the OS rejects it).

The untrusted WebView does not own every persisted field. The command preserves
the on-disk CLI-tools override and color-scheme preference, which have dedicated
commands, so a stale settings form cannot overwrite either value.

### `cmd_set_color_scheme(colorScheme: ColorScheme) -> ()`

Persists the requested `system`, `light`, or `dark` appearance preference,
updates native Tauri chrome and opaque WebViews, then emits
`color-scheme-changed`. Fresh settings follow the operating system; legacy
settings files missing this field retain the historical dark appearance.

### `cmd_set_watcher_enabled(input: { enabled: boolean }) -> ()`

Persists `settings.watcherEnabled` and starts or stops the source watcher immediately. See [watcher.md](./watcher.md).

### `cmd_toggle_palette() -> ()`

Show (and focus) or hide the floating command-palette window. Bound to the global accelerator (handled in Rust) and the View > Command Palette menu item; also callable from the UI.

### `cmd_show_main() -> ()`

Show + focus the main window and hide the palette. Used by palette navigation commands that route back into the main window (paired with the `hub-navigate` event).
The main renderer also calls it after resolving its color scheme, so first
display never reveals an incorrect theme.

### `cmd_rescan_resync() -> ()`

Recovery fallback: a full rescan + resync of every enabled tool (no newcomer auto-enable) plus the active workspace, then emits `sources-changed`. Use when the watcher is paused or projections look out of sync.

```typescript
type Settings = {
  sources: SourceConfig[];        // ordered by priority
  sharedRoot: string;             // deprecated; one-release fallback when sources is empty
  suitesPath: string | null;      // custom suite-store path, or null for the default
  watcherEnabled: boolean;
  editor: EditorPref;             // preferred editor for "open original"
  colorScheme: ColorScheme;       // 'system' | 'light' | 'dark'
  paletteShortcut: string;        // global accelerator, e.g. "Cmd+Alt+A"
  tools: ToolsSettings;
};

type ColorScheme = 'system' | 'light' | 'dark';

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
  kind: 'skill' | 'agent' | 'rule' | 'hook' | 'command';
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
the watcher and other suite writes via the shared projection transaction guard.
A **normal** suite re-syncs only its bound tools; the **base** suite re-syncs
**every** binding. Emits `sources-changed`
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
  preserveManual?: boolean; // default false — full reset; true keeps tracked manual extras
};

type ApplySuiteResult = {
  applyResult: ApplyResult;
  ruleSync: SyncRulesResult;
  hookSync: SyncHooksResult;
  skippedStale: number;          // present-source/unqualified refs with no match
  skippedAbsentSource: number;   // qualified refs whose source isn't on this machine (preserved)
  suite: { id: string; name: string };
};
```

`pnpm gen:types` remains the only supported way to update these bindings. Its
final fixed-path hygiene step normalizes trailing whitespace in
`src/types/generated/*.ts` with Node built-ins, so repeated ts-rs exports stay
diff-clean without adding a formatter dependency or accepting an untrusted
path.

Side effect: records a suite<->tool binding (`record(toolId, suiteId, manualItemIds)`,
upsert per tool) so a later `cmd_update_suite` re-syncs this tool with the stored
manual set, and backfills unqualified refs against the live scan. Both the
palette suite-apply flow and the Suites page flow through here. When tracked
manual extras exist outside the effective suite (base + selected), the UI
calls `cmd_suite_apply_preview` first and prompts the user to fully override or
keep manually added items before apply.

### `cmd_suite_apply_preview(input: ApplySuiteInput) -> string[]`

Returns tracked manual item ids for `toolId` that are still enabled and not in
the effective suite (after base merge and in-memory source backfill). Empty →
apply directly; non-empty → UI confirm dialog.

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

### `cmd_resync_suite_binding(toolId: ToolId) -> ApplySuiteResult`

Re-applies the tool's actual live binding as `selected suite ∪ current base ∪
manual extras` through the existing non-force suite pipeline. Persisted manual
extras are unioned with currently enabled extras, so drift recovery cannot drop
either set. The command returns projection, rule-sync, and hook-sync failures in
`ApplySuiteResult`, allowing the Manager to distinguish success, partial
success, and failure. It refreshes the stored manual set only after all three
write phases succeed; a partial result leaves the prior binding unchanged. The
shared projection transaction guard covers binding read through conditional
record so separate windows cannot interleave recovery with suite apply or
mutation re-sync. The command emits `sources-changed` after any completed
locked attempt, including when projection or sync writes succeeded but binding
persistence then returned a typed error. Emission does not mask that error: the
original `IpcError` still reaches the caller. Settings-load failures occur
before a recovery attempt and therefore do not emit.

Errors are typed: `suite_binding_not_found` when the tool has no binding and
`suite_not_found` when the binding's selected suite no longer exists. Neither
case changes the binding or any projection.

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

## Skill source commands (opt-in skills.sh source)

Gated behind `settings.skills.enabled`. Search runs through Rust (below) against
the keyless public skills.sh index — no API key. See
[skill-sources.md](./skill-sources.md).

### `cmd_skill_cli_check(input: { provider: string }) -> SkillCliStatus`

Probes a provider's installer tooling (for skills.sh: `npx`/Node). Advisory.

```typescript
type SkillCliStatus = { available: boolean; version: string | null; message: string };
```

Errors: `unknown_provider`.

### `cmd_search_skills(input: { provider, query, limit? }) -> SkillSearchHit[]`

Searches a provider's keyless public index. Runs in Rust (not a WebView `fetch`)
because the skills.sh search endpoint sends no CORS header. Queries under two
characters return `[]` without a request. Links are derived in Rust.

```typescript
type SkillSearchHit = {
  id: string;            // "{source}/{slug}"
  skillId: string;       // per-skill slug
  name: string;
  source: string;        // e.g. "owner/repo"
  installs: number;
  installRef: string;    // ref for `npx skills add`
  githubUrl: string | null;
  pageUrl: string;
};
```

Errors: `unknown_provider`, `skill_search`.

### `cmd_list_skill_favorites() -> SkillFavoritesState`

Reads the local starred-skills file (`~/.agentic-hub/skills-favorites.json` or
the configured override).

### `cmd_add_skill_favorite(favorite: SkillFavorite) -> SkillFavorite`

Upserts a favorite by `(provider, id)`, newest-first; stamps `starredAt`.

### `cmd_remove_skill_favorite(input: { provider: string; id: string }) -> null`

Unstars by `(provider, id)`; absent entries are a no-op.

### Skill install window

Skill install is the **one** explicit workspace write, and it runs in its own
dedicated `install` window (mirroring the `palette` window) so output can stream
live and the run can be cancelled. The workspace FAB opens the window; the window
loads the selection matrix and drives the streaming command per chosen skill.

#### `cmd_open_install_window(workspaceId: string) -> ()`

Stores the target [`InstallContext`](#install-context-changed) and builds/shows
the `install` window. If the window was already open, emits `install-context-changed`
so it re-reads the context. Errors: `workspace_not_found`, `window_build_failed`.

#### `cmd_take_install_context() -> InstallContext`

Read by the install window on mount to learn which workspace it targets.

```typescript
type InstallContext = { workspaceId: string; workspaceLabel: string };
```

Errors: `no_install_context`.

#### `cmd_install_skill_stream(input, onEvent: Channel<SkillInstallEvent>) -> ()`

Resolves + validates the workspace dir against the target store, spawns the
provider's install (`npx skills add <ref>`) with piped stdio, and streams output
over `onEvent` — one `line` per output line, then a terminal `done`. The child is
stored so `cmd_cancel_install` can kill it. On success the watcher is restarted
and `workspace-changed` is emitted so the read-only inventory re-scans. Resolves
when the run finishes (or is cancelled).

```typescript
type InstallSkillInput = {
  provider: string;
  installRef: string;          // validated owner/repo (or owner/repo/skill)
  workspaceId: string;
  toolIds: ToolId[];           // advisory; the CLI auto-detects agents
};
type SkillInstallEvent =
  | { kind: 'line'; stream: string; text: string }
  | { kind: 'done'; ok: boolean; cancelled: boolean };
```

Errors: `invalid_skill_ref`, `unknown_provider`, `skill_cli_missing`,
`install_failed`, `workspace_not_found`.

#### `cmd_cancel_install() -> ()`

Kills the in-flight install child (if any); the current stream then ends as
`cancelled`. The install-window close handler calls the same path so an abandoned
window never strands a running `npx`.

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

## Local usage tracing commands

### `cmd_usage_tracing_status() -> UsageTracingStatus`

Returns whether local tracing is enabled, whether the loopback collector is
healthy through an authenticated loopback probe, the configured collector port, the local SQLite path
(`~/.agentic-hub/usage/trace.db`), and the tools supported by the built-in tracer
hook.

### `cmd_set_usage_tracing_enabled(input: { enabled: boolean }) -> UsageTracingStatus`

Persists the opt-in tracing flag. When enabling, generates a local collector
token if needed, starts and verifies the `127.0.0.1` collector, and syncs Agentic Hub-managed
tracer hooks for the enabled capture tools. When disabling, stops the collector
and removes only the built-in tracer hook entries, preserving foreign hooks and
other Agentic Hub-managed hooks.

### `cmd_sync_usage_tracer_hooks() -> UsageTracerHooksSyncResult`

Reinstalls Agentic Hub-managed tracer hooks for the enabled capture tools and
recovers then verifies the loopback collector when tracing is enabled. Returns the
refreshed tracing status plus the tools that received an active tracer hook.
Use this after updating Agentic Hub or changing capture tools instead of
toggling tracing off and on. Restart each agentic tool afterward so it reloads
its hooks file.

### `usage-tracing-health-failed` event

Emitted once after the hourly checker cannot recover an enabled collector in
three restart-and-probe attempts. The typed payload contains the fixed attempt
count and a user-safe message. The main window uses it for the native desktop
notification and a persistent restart prompt; no raw collector error or token is
included.

### `cmd_query_usage_stats(input: { items: CapabilityItem[] }) -> UsageStats[]`

Queries local usage aggregates for the scanned skill and command rows. Unknown or
ambiguous events remain stored in the trace database but do not appear in
capability row counts.

### `cmd_query_usage_dashboard(input: { items: CapabilityItem[], range: UsageDateRange }) -> UsageDashboard`

Returns aggregated local usage metrics for the Statistics page: overview
counters, daily activity, breakdowns by kind/source tool/workspace, top-used
capabilities (joined with scan metadata), today's top-used capabilities
(`todayTopCapabilities`, filtered to the machine's local calendar day
regardless of `range`), and installed-but-unused rows. Terminal-event
filtering matches `cmd_query_usage_stats`. `range` is one of `last7Days`,
`last30Days`, `last90Days`, or `allTime`.

### `cmd_record_command_palette_usage(input: { capabilityId: string, pasted: boolean }) -> ()`

Records a `CommandPaletteUse` event when the user copies or pastes a command from
the palette. No-op when local tracing is disabled. `pasted` is `true` when the
palette also posted the body into the focused app.

## Session Explorer commands

Read-only access to the local session history of Codex, Claude Code, and Cursor.
A metadata-only SQLite index is the eventual plan (see
[session-explorer.md](./session-explorer.md)) but is **not built yet** —
`cmd_list_sessions` currently re-derives this shape live from each tool's
source on every call. Transcript bodies are always read on demand and never
persisted, regardless of the index's status.

### `cmd_list_sessions(input: ListSessionsInput) -> SessionSummary[]`

Returns normalized session metadata for the enabled tools, newest activity
first, filtered by tool, workspace, a title/metadata query, and `range`.
Sessions belonging to a tool disabled in Settings are excluded. `range` isn't
just a post-filter: Claude/Codex skip a file's content entirely when its mtime
falls outside `range` (a cheap stat, not a parse), and Cursor skips a composer
once its resolved last-activity timestamp falls outside `range` — this is what
makes the UI's `today` default actually reduce work instead of only
trimming an already-fully-read list.

```typescript
type ListSessionsInput = {
  tools?: ToolId[];        // omitted/undefined = all enabled tools; [] matches none, not "all"
  workspace?: string;      // tildified path filter, or null
  query?: string;          // matches title + metadata; under 2 chars = no filter
  range: UsageDateRange;   // required — no server-side default; UI defaults to 'today'
};

type SessionSummary = {
  sessionKey: string;      // "<tool>:<native session id>"
  tool: ToolId;            // 'codex' | 'claude' | 'cursor'
  title: string;           // best-available human title (metadata only)
  workspace: string | null;// tildified cwd/folder, or null (unknown)
  gitBranch: string | null;
  model: string | null;
  startedAt: string | null;// ISO 8601, or null if unresolvable
  updatedAt: string | null;// ISO 8601, or null if unresolvable
  messageCount: number;
  sourcePath: string;      // absolute file path, or a store-specific locator (Cursor's composer id)
};
```

### `cmd_get_session(input: { sessionKey: string }) -> SessionMessage[]`

Resolves `sessionKey` against a fresh, unrestricted (`AllTime`) listing —
independent of whatever `range` the Sessions pane is currently viewing, so a
session the UI already knows about always resolves — then reads its
transcript on demand from the source (JSONL lines, or every
`bubbleId:<composerId>:*` row for Cursor) and returns role-attributed
messages. Nothing is written back anywhere. Errors: `session_source_unavailable`
when the session key no longer resolves or the source can't be read.

```typescript
type SessionMessage = {
  role: 'user' | 'assistant' | 'tool' | 'system';
  text: string;
  toolName: string | null;
  timestamp: string | null; // ISO 8601 when the source records one
};
```

### `cmd_reindex_sessions(input: { tool?: ToolId }) -> SessionsStatus` (planned — not built yet)

Recovery fallback: forces a full re-index, or a single-tool re-index when `tool`
is set. Upserts changed entries and prunes rows whose source disappeared.
Returns the refreshed status.

### `cmd_sessions_status() -> SessionsStatus` (planned — not built yet)

Returns the index path, per-tool indexed counts, and the last-index time.

```typescript
type SessionsStatus = {
  dbPath: string;
  indexedByTool: Record<ToolId, number>;
  lastIndexedAt: string | null; // ISO 8601
};
```

The Open and Reveal actions reuse `cmd_open_path` / `cmd_reveal_path` below;
Session Explorer adds no new open/reveal surface.

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

### `cmd_open_url(input: { url: string }) -> ()`

Opens an external `http`/`https` URL in the default browser. Anchor navigation
(`<a target="_blank">`) is a no-op inside the WebView, so external links route
through Rust. The scheme/host is validated server-side
(`open_targets::is_safe_external_url`). Errors: `url_not_openable`, `open_failed`.

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

Emitted by the palette window when a go-to (locate) result is chosen. The main window routes to Manager, switches to the requested scope — for `workspace` it activates the owning workspace (loading its inventory) and namespaces the row id with `ws::`; for `global` it uses the raw id — and flags the matching matrix row so the `Matrix` expands its folders, scrolls to it, and highlights it briefly. The palette then surfaces the main window via `cmd_show_main`.

```typescript
type LocateRequest =
  | { scope: 'workspace'; workspaceId: string; itemId: string }  // raw (non-namespaced) item id
  | { scope: 'global'; itemId: string };
```

### `hub-watcher-changed`

Emitted by the palette window after its "Pause/Resume watching" action persists the new state via `cmd_set_watcher_enabled` (payload: the new boolean). The main window's manager store updates its `watching` flag so the header toggle stays in sync without a re-fetch.

### `install-context-changed`

Emitted (no payload) to the `install` window when it is reopened for a different
workspace while already open. The window re-reads its context via
`cmd_take_install_context`.

### `settings-changed`

Emitted globally when `cmd_save_settings` succeeds.

```typescript
type SettingsChangedEvent = {};  // empty; receivers re-fetch
```

### `color-scheme-changed`

Emitted globally when `cmd_set_color_scheme` persists a preference. Every
window applies the preference locally; windows following the system also react
to the browser `prefers-color-scheme` media query for live OS changes.

```typescript
type ColorSchemeChangedEvent = 'system' | 'light' | 'dark';
```

### `sessions-changed` (planned — not built yet)

Emitted (no payload) after a scoped or full session re-index completes following
a change under a tool's session store. The Sessions pane listens and refreshes
its list in place. Depends on the watcher-driven incremental re-index, which
isn't built yet (see [session-explorer.md](./session-explorer.md)'s delivery
slices); today the Sessions pane only refreshes via its manual Refresh button.

```typescript
type SessionsChangedEvent = {};  // empty; receivers re-fetch
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

Every command listed here must appear in `crates/agentic-hub/capabilities/default.json` under the main window's permission list. The Suite Manager window inherits the same permission set in v1 to keep things simple; a future tighter capability split is possible.

The floating `palette` and `install` windows have their own capability files
(`palette.json`, `install.json`) granting `core:default` plus the window
show/hide/focus (and, for `install`, close/start-dragging) and event
emit/listen they need. Custom app commands (`cmd_*`) are cross-window and need no
plugin permission, so the install window's `cmd_*` calls work without listing.

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
