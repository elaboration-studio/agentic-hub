# Architecture

This document explains **why** Agentic Hub is built the way it is.

## The core idea

Agentic Hub is a Tauri 2.x desktop app that owns one job: projecting a single shared agentic capability tree (`~/.agentic/{skills,agents,rules}`) into the disparate filesystem conventions of four AI coding tools (Codex, Claude Code, Cursor, OpenClaw), safely and reversibly. Every other concern — UI, suites, workspace patches, scaffolding — is built on top of that one engine.

The hard problem is not the UI. It is that each tool has a different filesystem contract:

- Claude Code only scans the top level of `~/.claude/skills/`, so nested categories must collapse to basename.
- Cursor loads agent files into memory at launch, so symlinks are unreliable — we use managed file copies with stale detection.
- Codex, Claude, and OpenClaw read rules from a single instruction file (`AGENTS.md`, `CLAUDE.md`, `SOUL.md`) — we maintain a marker-delimited managed block inside those files.
- Workspace-scoped projections (`<ws>/.cursor/`, `<ws>/.claude/`, `<ws>/.agents/`) must use hard copy because many work repos sync over Dropbox / iCloud where symlinks are flaky.

The strategy: a Rust core (`agentic-core`) owns the entire projection engine — scan, plan, apply, rule sync, suite store, workspace patch — as testable pure functions over a filesystem boundary. The Tauri shell (`agentic-hub` bin crate) is a thin IPC + window layer. The React UI is a view over IPC-served state and is never trusted with filesystem operations.

```
                            +-----------------------------+
                            |     React + Vite UI         |
                            |  (main window + Suite Mgr)  |
                            +--------------+--------------+
                                           |
                              invoke() / event channels
                                           |
                            +--------------v--------------+
                            |    Tauri 2.x shell          |
                            |  agentic-hub bin crate      |
                            |  - commands                 |
                            |  - window mgmt              |
                            |  - capability files         |
                            |  - dialog / store plugins   |
                            +--------------+--------------+
                                           |
                            +--------------v--------------+
                            |   agentic-core (Rust lib)   |
                            |                             |
                            |   settings   scanner        |
                            |   adapters   planner        |
                            |   applier    rule_sync      |
                            |   suites     workspace      |
                            +--------------+--------------+
                                           |
        +----------------+-----------------+----------------+----------------+
        |                |                 |                |                |
   ~/.agentic       ~/.codex          ~/.claude        ~/.cursor       ~/.openclaw
   (shared root)   AGENTS.md          CLAUDE.md        skills/         workspace/SOUL.md
                                                       agents/         skills/, agents/
                                                       rules/
```

## System overview

Agentic Hub runs as a single-user desktop app. There is no server, no remote sync, and no auto-update in v1. The only network egress is opt-in anonymous usage telemetry (off by default; see below). The system boundary is the local filesystem on one machine.

External surfaces:

- **Shared root** — user-owned directory tree, default `~/.agentic`, source of truth for capability definitions
- **Tool homes** — `~/.codex`, `~/.claude`, `~/.cursor`, `~/.openclaw` — owned by the respective AI tools, written into by the projection engine with safe semantics
- **App-owned data** — `~/.agentic-hub/config.json` (settings), `~/.agentic-suites.json` (suites, parity path with VS Code extension), `~/.agentic-hub/state.json` (workspace target store). Workspace scope is read-only and writes no per-workspace data.

No network calls in v1 except opt-in usage telemetry (Aptabase), which is off by default and sends only coarse lifecycle events from the Rust core when the user enables it.

## Domain architecture docs

- [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) — Tauri 2.x capability model, FS scoping, runtime scope additions, Windows symlink constraint. Split out because Tauri's capability JSON contract is a distinct trust-boundary surface that does not belong in the root system story.
- [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md) — projection engine deep dive: scan → plan → apply → rule-sync data flow, per-tool layout matrix, Cursor managed-copy lifecycle, Claude flat-layout collision rules. Split out because the projection engine is the bulk of the system and benefits from its own dedicated reading path.
- [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md) — read-only workspace inventory: per-tool scan of a project's own dirs, the target store, watch + `workspace-changed` refresh. Split out because workspace scope is the inverse of global projection (read-only audit, no writes) and has its own lifecycle.

## Related detailed docs

- [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md) — full IPC command surface and event schemas
- [docs/tech/reference/tool-adapter-matrix.md](docs/tech/reference/tool-adapter-matrix.md) — per-tool projection matrix
- [docs/tech/reference/shared-root-contract.md](docs/tech/reference/shared-root-contract.md) — shared root filesystem contract

## Repository layout

The repo is a Tauri 2.x project with a Rust workspace and a pnpm-managed UI. Layout is part of the architectural story because the Rust core / Tauri shell split is a hard boundary.

```
agentic-hub/
  PRODUCT.md                  product requirements
  ARCHITECTURE.md             this doc
  ARCHITECTURE.permissions.md
  ARCHITECTURE.projection.md
  ARCHITECTURE.workspace.md
  AGENTS.md                   agent guide for future Codex/Claude work
  README.md
  docs/                       detailed docs (features, tech.modules, tech.reference, tech.development)

  Cargo.toml                  Rust workspace
  crates/
    agentic-core/             pure domain crate: scan/plan/apply/sync/suites/workspace-inventory
    agentic-hub/              Tauri bin crate: commands, window mgmt, capability files

  package.json                pnpm workspace
  components.json             shadcn/ui config (New York, @/ aliases)
  src/                        React + Vite + TS UI
    main.tsx
    App.tsx                   thin shell: layout + hash routing
    index.css                 Tailwind v4 entry + design tokens (see DESIGN.md)
    ipc.ts                    typed wrappers around Tauri invoke()
    shared.tsx                shared constants + pure helpers
    lib/                      cn() and UI utilities
    state/                    Zustand stores (manager, suites, workspace)
    components/
      ui/                     shadcn/ui primitives
      layout/                 Header, ActionBar
      manager/                Matrix, ToolCells, EmptyState, ConflictDialog
      config/                 ConfigPage
      suites/                 SuitesPage
      workspace/              WorkspacePanel
      palette/                CommandPalette + command-provider registry
    types/                    TS types mirrored from agentic-core via codegen

  src-tauri/                  Tauri config + capability files
    tauri.conf.json
    capabilities/
      default.json
      workspace.json
    icons/

  resources/
    agentic-demo/             bundled demo scaffold tree

  tests/
    integration/              cross-crate tests against tmp directories
```

## Components and responsibilities

| Component | Layer | Responsibility |
|-----------|-------|----------------|
| `settings` | core | Load/save `~/.agentic-hub/config.json`; normalize tilde-paths; provide defaults; validate tool target paths |
| `scanner` | core | Walk `<sharedRoot>/{skills,agents,rules}`; produce `CapabilityItem[]`; capture per-item validation errors |
| `adapter_registry` | core | Per-tool declarative adapter: target paths, projection kind per kind, layout strategy (`flat`/`nested`); single owner of basename-vs-relative-path decisions |
| `planner` | core | Inspect current per-tool disk state; diff against desired-state map; emit `PlannedOperation[]`; no FS writes |
| `applier` | core | Execute plan: create / replace / remove symlinks and managed copies; refuse to overwrite real files/dirs; per-op failure isolation |
| `rule_sync` | core | Rewrite managed marker block in `AGENTS.md` / `CLAUDE.md` / `SOUL.md`; strip frontmatter; preserve unmanaged content |
| `suite_store` | core | CRUD over `~/.agentic-suites.json`; atomic write; UUID generation; stale-reference validation |
| `workspace_inventory` | core | Read-only scan of a workspace's own per-tool dirs; produce `CapabilityItem[]` + present-only `ToolCapabilityState[]`; no FS writes |
| `scaffold` | core | Materialize bundled demo tree at `sharedRoot`; merge / overwrite modes |
| `agentic-hub` bin | shell | Tauri commands wired to core; window mgmt; capability JSON; dialog + store plugin wiring |
| React UI | shell | View + staging state; never calls FS directly; communicates only through typed IPC wrappers |

## Key design decisions

### Rust core, not TypeScript core in Node sidecar

The VS Code extension runs filesystem ops in Node. Re-using that code via a Tauri Node sidecar would have been faster to start but locks us to Node's symlink semantics, makes binary size larger, and creates a runtime boundary inside the app for no architectural benefit. A native Rust core is ~10MB smaller in distribution, gives precise control over symlink semantics, and uses Tauri's first-class command model. Types are mirrored to TS via codegen (`ts-rs` or `specta`) so the IPC contract stays single-sourced.

### React + Vite + TypeScript

Chosen for ecosystem maturity and team familiarity. Tauri is framework-agnostic; this decision is reversible per-screen but unlikely to be reversed. State is held in Zustand stores; data fetching is direct `invoke()` calls (no React Query) because the data is local and small.

### Stage-then-apply, not live edit

Every UI toggle modifies an in-memory staged state, not disk. Apply is explicit. This was a hard rule in the VS Code extension and is preserved verbatim. It is the single biggest correctness lever: it makes destructive operations require an explicit user action.

### Filesystem is source of truth

No shadow database. The planner re-inspects disk state on every plan. The scanner re-reads the shared root on every scan. The only persistence layers are: settings, suites, workspace target store, and per-workspace manifests. Each of those persists user intent, not filesystem-derived state.

### Capability files, not legacy allowlist

Tauri 2.x uses fine-grained capability JSON files. We declare scoped FS access for known tool homes and the shared root statically; user-picked workspace dirs add scope at runtime via the dialog plugin's allow-on-pick pattern. The WebView is never granted `shell:execute` or unscoped FS access. See [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md).

### Markdown markers preserved from VS Code extension

`<!-- agentic-hub:start -->` / `<!-- agentic-hub:end -->` markers match the rebranded VS Code extension verbatim for migration parity. Users coming from the extension keep working without touching their instruction files.

## Stack decisions

### Tauri 2.x

The product is a desktop app that needs scoped FS access, native menus, dialog pickers, and small binary size. Tauri 2.x gives all of that with a smaller footprint than Electron and a first-class Rust core that matches our filesystem-heavy workload. The capability model in Tauri 2.x is also stricter than 1.x, which matters because this app writes into multiple tool homes.

Rejected: Electron (bundle size, no Rust core), Wails (smaller ecosystem, less mature 2.x story), pure CLI (no UX win over bash for the target user).

### React + Vite + TypeScript

Boring default. Vite is fast in dev, React's component model fits the panel + inspector layout, TypeScript catches IPC payload mismatches at compile time. We pay nothing in flexibility — switching to Solid or Svelte for one screen is trivially possible.

Rejected: SolidJS (smaller bundle but smaller ecosystem), Svelte (good fit but team familiarity lower), vanilla (more boilerplate, no upside).

### Tailwind v4 + shadcn/ui for styling

The UI is components-first: shadcn/ui primitives (`src/components/ui/`) composed into feature views, styled with Tailwind v4 utility classes over a CSS-variable token set. No hand-rolled buttons, inputs, modals, or menus. Tailwind v4 uses the `@tailwindcss/vite` plugin and a CSS-first config (a single `@import "tailwindcss";` in `src/index.css`, no `tailwind.config.js`). shadcn components are owned source under `src/components/ui/`, not a runtime dependency — we can edit them freely. Radix primitives (via the unified `radix-ui` package) back the interactive components. Design tokens and component conventions are the source of truth in [DESIGN.md](DESIGN.md); the app supports matched light and dark palettes through a persisted `ColorScheme` preference.

Rejected: a bespoke CSS file (the prior approach — drifted to ~1200 lines with no component contract), CSS-in-JS (runtime cost, no token story), a heavyweight component kit like MUI (opinionated theming fights a custom token system, larger bundle).

### Zustand for state

The data model is small and local. We do not need React Query (no server). Zustand keeps stores tiny and focused: `manager` owns the scan/inspect/stage/apply loop (and, in read-only mode, the workspace inventory render), `suites` owns suite CRUD + draft, `workspace` owns the workspace target list and delegates inventory loading to `manager`. The thin `App.tsx` shell only wires layout, hash routing, and Tauri event listeners; views read the stores directly, so prop-drilling is minimal. Cross-store refresh (e.g. "a suite was created") flows through Tauri events. Action errors surface as Sonner toasts; the initial-load failure surfaces as a full-page alert.

Rejected: Redux (too much ceremony), React Query (no HTTP), Jotai/Recoil (no benefit over Zustand at this scale).

### `ts-rs` for type sharing

Rust types in `agentic-core` get a `#[derive(TS)]` to generate matching TS types in `src/types/`. Single source of truth for IPC payloads. Codegen runs in `cargo test`.

Rejected: specta (newer, less stable), hand-mirroring (drift risk).

### `tauri-plugin-dialog`, `tauri-plugin-store`, `tauri-plugin-shell` (disabled)

Dialog for workspace folder picker. Store for the small persisted runtime state that does not warrant a custom file format. Shell is explicitly disabled — the WebView must never get arbitrary command execution.

## Key flows

### Open main window flow

```
User launches Agentic Hub
  -> Tauri shell creates main window
  -> UI mounts CapabilityManager panel
  -> UI invokes cmd_load_settings()
       -> settings::load() reads ~/.agentic-hub/config.json (or defaults)
  -> UI invokes cmd_scan(sharedRoot)
       -> scanner::scan() walks shared root, returns ScanResult
  -> UI invokes cmd_inspect(scanResult, settings.tools)
       -> for each enabled tool:
            adapter_registry::resolve() -> target paths
            planner::inspect_current_state() -> ToolCapabilityState[]
       -> returns Map<ToolId, ToolCapabilityState[]>
  -> UI renders capability list + per-tool states
```

This flow works because the scan, adapter resolution, and state inspection are all idempotent reads. Any of them can be re-run on refresh without state machine complexity.

### Window lifecycle (close vs quit)

Agentic Hub is a standalone background app. Closing the window (red traffic-light button or Cmd+W) does **not** quit — the shell intercepts `WindowEvent::CloseRequested`, hides the **application** (macOS `NSApp hide:` via `AppHandle::hide()`), and calls `prevent_close()`. The process stays alive, the source watcher keeps reconciling, and window state is preserved. Because the app is hidden (not just the window ordered out), **Cmd+Tab** and clicking the Dock icon both re-activate the app and restore the window the macOS-native way; `RunEvent::Reopen` additionally re-shows and focuses on Dock click unless a pending or active palette presentation blocks the racing reopen. Palette dismissal restores the prior app-hidden/focused/external state instead of leaving the process visible with no restorable window. The only intended hard exit is **Cmd+Q**, which goes through the **native menu's** `PredefinedMenuItem::quit` and bypasses the close handler to terminate the process.

### Command palette window, global shortcut, and native menu

A second window (label `palette`) is a borderless, transparent, floating panel — an Alfred-style command palette. It is created hidden at launch and toggled by a user-configurable global accelerator (`Settings.paletteShortcut`, default `Cmd+Alt+A`) registered via `tauri-plugin-global-shortcut`. The palette hides on `WindowEvent::Focused(false)` (Alfred-style dismiss). Transparency requires the `tauri` `macos-private-api` feature and `macOSPrivateApi: true` in config. **On macOS the window is subclassed to a non-activating `NSPanel`** (via `tauri-nspanel`, a macOS-only git dependency) with a non-activating style mask and `FullScreenAuxiliary | CanJoinAllSpaces` collection behavior, so it floats over other apps' full-screen Spaces without stealing focus or switching Spaces — a plain `NSWindow` cannot do this ([tauri#11488](https://github.com/tauri-apps/tauri/issues/11488)). Before native centering, the panel moves to `NSScreen.mainScreen` (the display containing the focused window), so multi-display summon follows the user's current context. All panel objc operations run on the main thread (`AppHandle::run_on_main_thread`); `palette_presentation.rs` assigns revision tokens before that hop so stale show/hide callbacks cannot overwrite newer intent, and records whether the hub was app-hidden, focused, or behind an external app for dismissal restoration. Non-macOS platforms fall back to an always-on-top, all-workspaces window. The `unsafe` FFI is encapsulated in `tauri-nspanel`, keeping this crate `unsafe`-free. One React bundle serves both windows: `main.tsx` branches on the window label to render `<CommandPalette/>` vs `<App/>`. The palette searches resources flat and opens the original file via the existing `cmd_open_path` allowlist; a command-provider registry (`components/palette/commands.ts`) keeps it extensible. The shell also installs a native menu (`menu.rs`): App (About, Settings `Cmd+,`, Hide, Quit), Edit, View (Command Palette), Window. Settings emits `menu-open-config`; palette navigation emits `hub-navigate`; both are handled by the main window. See [docs/features/command-palette.md](docs/features/command-palette.md).

### Apply changes flow

```
User toggles items + clicks Apply
  -> UI computes desiredEnabledByItemId for focused tool
  -> UI invokes cmd_plan({ toolId, desiredMap })
       -> planner::build_plan(toolId, items, currentStates, desiredMap)
       -> returns PlannedOperation[]
  -> UI shows plan preview (optional in v1; mandatory before apply if any skip_conflict)
  -> User confirms
  -> UI invokes cmd_apply({ operations })
       -> applier::apply(plan) executes:
            for each operation:
              create_link / remove_link / replace_link
              create_managed_copy / replace_managed_copy / remove_managed_copy
              skip_conflict (no FS write)
            emit progress event after each op
       -> returns ApplyResult { counts, errors[] }
  -> if tool uses markdown_section_sync:
       -> UI invokes cmd_sync_rules({ toolId, items, states })
            -> rule_sync::sync_markdown_rules()
  -> UI invokes cmd_inspect() again
  -> UI renders refreshed state + result summary toast
```

The two-phase plan-then-apply is critical. The plan is computed once and shown before any FS write. If the user wants a dry-run, the plan output is the dry-run. The apply step is deterministic given the plan.

### Apply suite flow

```
User selects suite + clicks Apply Suite
  -> UI confirms with suite name + tool name
  -> UI invokes cmd_apply_suite({ toolId, suiteId })
       -> suite_store::get(suiteId)
       -> scanner::scan()
       -> for every scanned item: desiredEnabled = item.id IN suite.capabilities
       -> planner::build_plan() with full-coverage desired map
       -> applier::apply()
       -> rule_sync::sync_markdown_rules() if applicable
       -> count stale suite refs (in suite, not in scan)
       -> return ApplyResult + staleCount
  -> UI re-inspects + renders
```

Full-coverage desired map is the key: every scanned item gets a desired state, so the plan is a true reset. This reuses the existing plan/apply pipeline with zero new mutation code.

### Workspace inventory flow (read-only)

```
User picks / activates a workspace in the left rail
  -> UI invokes cmd_scan_workspace({ workspaceId })
       -> resolve target dir from WorkspaceTargetStore
       -> workspace_inventory::scan_workspace(ws, WORKSPACE_TOOL_IDS):
            for each tool (Codex / Claude / Cursor):
              build create_workspace_adapter(tool, ws)
              walk skills_path  -> skill:<rel>
              walk agents_path  -> agent:<rel>
              walk cursor rules_path -> rule:<rel>
              instructions_path (AGENTS.md / CLAUDE.md) if present -> rule:<file>
            dedupe items by id across tools; emit one Enabled state per present (tool, item)
       -> returns WorkspaceInventory { items, states, errors }
  -> UI renders the same matrix in read-only mode (static present cells, inert aggregates)
```

No writes occur. The watcher re-subscribes to the active workspace's tool dirs and emits `workspace-changed`, which re-runs the scan. See [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md).

## Data model

### CapabilityItem

```rust
pub enum CapabilityKind { Skill, Agent, Rule, Hook, Command }

pub struct CapabilityItem {
    pub id: String,                  // e.g. "skill:dev/repo-research", "command:review/code-review.md"
    pub kind: CapabilityKind,
    pub name: String,                // basename or skill folder name
    pub source_path: PathBuf,        // absolute path under shared root
    pub relative_path: PathBuf,      // path relative to <shared>/<kind>/
    pub valid: bool,
    pub validation_errors: Vec<String>,
}
```

Commands are the fifth kind: file-based, nested markdown under `<root>/commands/` (the same shape as agents/rules, not folder-with-marker like skills). They project into each tool's slash-command directory (symlink for Cursor/Codex/OpenStandard, managed copy for Claude; OpenClaw unsupported) and are searchable in the palette — Enter copies the body to the clipboard, Alt+Enter opens the source file.

### ToolAdapter

```rust
pub struct ToolAdapter {
    pub id: ToolId,                          // Codex | Claude | Cursor | OpenClaw
    pub enabled: bool,
    pub skills_path: PathBuf,
    pub agents_path: PathBuf,
    pub rules_path: PathBuf,
    pub commands_path: Option<PathBuf>,      // slash-command dir (None for OpenClaw)
    pub instructions_path: Option<PathBuf>,  // for markdown_section_sync tools
    pub skill_layout: Layout,                // Flat | Nested
    pub agent_layout: Layout,
    pub rule_projection: RuleProjectionKind, // LinkSync | FileSync | MarkdownSectionSync
}
```

### PlannedOperation

```rust
pub enum OperationKind {
    CreateLink, RemoveLink, ReplaceLink,
    CreateManagedCopy, RemoveManagedCopy, ReplaceManagedCopy,
    SkipConflict,
}

pub struct PlannedOperation {
    pub tool: ToolId,
    pub item_id: String,
    pub target_path: PathBuf,
    pub source_path: Option<PathBuf>,
    pub kind: OperationKind,
    pub reason: String,
}
```

### Settings

```rust
pub struct Settings {
    pub shared_root: PathBuf,
    pub tools: ToolsSettings,
}

pub struct ToolsSettings {
    pub codex: ToolSettings,
    pub claude: ToolSettings,
    pub cursor: ToolSettings,
    pub openclaw: ToolSettings,
}
```

Persisted at `~/.agentic-hub/config.json`. JSON schema mirrors the VS Code extension's `e-studio-copilot.agentic.*` settings 1:1 — fields are dotted-path-flattened in VS Code but nested in our JSON file because the JSON loader is our own.

### SuiteDefinition

```rust
pub struct SuiteDefinition {
    pub id: String,                    // UUID
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<String>,     // capability IDs
    pub created_at: String,            // ISO 8601
    pub updated_at: String,
}
```

Persisted at `~/.agentic-suites.json` (preserved path from VS Code extension).

### WorkspaceTarget

```rust
pub struct WorkspaceTarget {
    pub id: String,
    pub label: String,
    pub dir: PathBuf,
    pub last_used_at: String,          // ISO 8601
}
```

Persisted in `~/.agentic-hub/state.json` (`workspaceTargets` + `workspaceActiveId`). Workspace scope keeps no per-project manifest — the inventory is recomputed on demand by scanning the project's own tool dirs.

## Security model

Trust boundary: the WebView is **untrusted**. The Rust core is **trusted**.

- All filesystem mutations happen in `agentic-core`, never in the WebView
- Tauri capability files scope FS access to a known allowlist; the dialog plugin adds runtime scope for user-picked workspace dirs only
- `shell:execute` is never granted to the WebView
- All target paths normalized to absolute paths via `path.canonicalize()` before validation
- Workspace scope is read-only: it scans a project's own tool dirs and writes nothing, so there is no write-side boundary to enforce there
- No auto-update endpoint pinging in v1. The only remote egress is opt-in usage telemetry (Aptabase) — off by default, sent from the Rust core only, gated on consent at runtime; the WebView never calls out

See [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) for the full capability model.

## Observability and operations

- Apply result summary always shown in the UI (created / removed / replaced / refreshed / skipped / errors counts)
- Optional verbose log at `~/.agentic-hub/log/agentic-hub.log` (rotating, capped at 10MB), enabled via a debug toggle in settings
- Tauri DevTools enabled only in dev builds (`cfg(debug_assertions)`)
- No background jobs in v1 — no watchers, no schedulers, no auto-refresh
- No external observability; this is a personal-tool app

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| `cmd_load_settings` | Config file malformed | Settings load fails | Surface error, fall back to defaults, do not auto-overwrite the user's file |
| `scanner` | Shared root missing or non-dir | No items returned | Empty-state UI with "Scaffold Demo Resources" affordance |
| `scanner` | Shared root contains huge vendor folder | Scan slow | Constrain walk to `<root>/{skills,agents,rules}` only; ignore other top-level dirs |
| `adapter_registry` | Tool target path missing | Adapter reports unavailable | UI marks tool tab disabled; no apply for that tool |
| `planner` | Real file at target path | `skip_conflict` planned op | Surface in result summary; user must remove file manually |
| `planner` | Flat-layout basename collision | First-id-wins, others get `skip_conflict` | Surface; user renames source under `~/.agentic/` |
| `applier` | Permission denied mid-apply | Single op fails | Continue plan; report failure in result; partial apply is acceptable |
| `applier` | Shared source removed between plan and apply | Single op fails | Report in result; user re-scans |
| `rule_sync` | Instruction file is a directory | Conflict reported | No write; surface error |
| `rule_sync` | Markers malformed (start without end) | `broken` state | No rewrite until user fixes file manually |
| `suite_store` | Dotfile malformed JSON | List unavailable | Surface error; preserve in-memory state; do not auto-overwrite |
| `suite_store` | Dotfile write permission denied | Save fails | Surface error; preserve in-memory state |
| `workspace_inventory` | Active workspace dir was deleted | `workspace_not_found` / empty scan | Surface error; rail lets the user remove the stale target |
| `workspace_inventory` | A tool dir is unreadable | Per-dir `ScanError` | Skip that dir; record in `errors`; continue scanning the rest |
| Tauri capability denial | App tries to read outside scope | Op fails at IPC layer | Bubble error to UI; user can adjust scope in settings or pick a different workspace |

No failure mode in v1 is silent.

## What's intentionally not here

- **Background filesystem watchers.** Scan on open + manual refresh only. Watchers add complexity, race conditions, and battery cost for a use case where the user explicitly knows when they changed something.
- **Cloud sync of suites.** Personal tool, single machine. Multi-machine sync is a follow-on.
- **Team capability libraries.** This is a single-user app. Team sharing is a different product.
- **Auto-detection of installed tools.** Settings are explicit. Auto-detection adds magic that breaks when a tool path changes.
- **Workspace target for OpenClaw.** Out of scope per the VS Code feature spec; revisit once OpenClaw's project-level scan path stabilizes.
- **Rich preview of skill / agent content.** This is a manager, not an editor. Users edit content in their existing editor.
- **Workspace write-back.** Workspace scope is a read-only audit. The hub never copies capabilities into a project; it only reports what the project's tools already have.
- **Auto-update.** `tauri-plugin-updater` is supported but requires a signed manifest endpoint. Deferred until v1 public release.
- **Telemetry.** None. Personal-tool app, single user.
- **E-Studio remote sync.** Belongs in `e-studio-copilot`. Not part of this product.

## Open questions

- **Cross-window event channel granularity.** When Suite Manager creates / edits / deletes a suite, the main window's suite dropdown must refresh. Two options: (1) Tauri global event broadcast (`emit_all("suite-store-changed")`); (2) the main window polls on focus. Default for v1: global event broadcast. Validate during M2.
- **Type codegen toolchain.** `ts-rs` is the boring default. `specta` is newer and arguably more ergonomic. Decide at M0 scaffolding time; either is reversible.
- **macOS code-signing.** Required for Gatekeeper acceptance without user override. Decide before M4 launch hardening.
- **`tauri-plugin-store` vs custom JSON.** The store plugin gives us atomic writes for free for small state. We may use it for `state.json` (workspace target store) and keep `config.json` + `agentic-suites.json` as custom JSON because their schemas are stable and we want migration control. Decide at M0.
