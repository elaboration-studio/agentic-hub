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

Agentic Hub runs as a single-user desktop app. There is no server, no remote sync, no telemetry, no auto-update in v1. The system boundary is the local filesystem on one machine.

External surfaces:

- **Shared root** — user-owned directory tree, default `~/.agentic`, source of truth for capability definitions
- **Tool homes** — `~/.codex`, `~/.claude`, `~/.cursor`, `~/.openclaw` — owned by the respective AI tools, written into by the projection engine with safe semantics
- **App-owned data** — `~/.agentic-hub/config.json` (settings), `~/.agentic-suites.json` (suites, parity path with VS Code extension), `~/.agentic-hub/state.json` (workspace target store), `<ws>/.agentic-hub/workspace-patch.json` (per-workspace manifest)

No network calls in v1.

## Domain architecture docs

- [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) — Tauri 2.x capability model, FS scoping, runtime scope additions, Windows symlink constraint. Split out because Tauri's capability JSON contract is a distinct trust-boundary surface that does not belong in the root system story.
- [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md) — projection engine deep dive: scan → plan → apply → rule-sync data flow, per-tool layout matrix, Cursor managed-copy lifecycle, Claude flat-layout collision rules. Split out because the projection engine is the bulk of the system and benefits from its own dedicated reading path.
- [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md) — workspace-patch hard-copy semantics, manifest cycle, suite-apply integration, out-of-workspace path guard. Split out because workspace scope uses a different filesystem contract (hard copy + manifest) than global scope (symlinks + managed copies) and has its own lifecycle.

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
    agentic-core/             pure domain crate: scan/plan/apply/sync/suites/workspace-patch
    agentic-hub/              Tauri bin crate: commands, window mgmt, capability files

  package.json                pnpm workspace
  src/                        React + Vite + TS UI
    main.tsx
    App.tsx
    panels/
      CapabilityManager/      main window
      SuiteManager/           suite manager window
    components/
    ipc/                      typed wrappers around Tauri invoke()
    state/                    Zustand stores
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
| `workspace_patch` | core | Hard-copy suite apply into workspace; manifest write/read; cleanup cycle; out-of-workspace guard |
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

`<!-- e-studio-agentic-rules:start -->` / `<!-- e-studio-agentic-rules:end -->` markers are preserved verbatim for migration parity. Users coming from the VS Code extension keep working without touching their instruction files. Rename is deferred — see Open Questions in [PRODUCT.md](PRODUCT.md).

## Stack decisions

### Tauri 2.x

The product is a desktop app that needs scoped FS access, native menus, dialog pickers, and small binary size. Tauri 2.x gives all of that with a smaller footprint than Electron and a first-class Rust core that matches our filesystem-heavy workload. The capability model in Tauri 2.x is also stricter than 1.x, which matters because this app writes into multiple tool homes.

Rejected: Electron (bundle size, no Rust core), Wails (smaller ecosystem, less mature 2.x story), pure CLI (no UX win over bash for the target user).

### React + Vite + TypeScript

Boring default. Vite is fast in dev, React's component model fits the panel + inspector layout, TypeScript catches IPC payload mismatches at compile time. We pay nothing in flexibility — switching to Solid or Svelte for one screen is trivially possible.

Rejected: SolidJS (smaller bundle but smaller ecosystem), Svelte (good fit but team familiarity lower), vanilla (more boilerplate, no upside).

### Zustand for state

The data model is small and local. We do not need React Query (no server). Zustand keeps stores tiny and per-window. Suite Manager window and main window each have their own store. Cross-window state (e.g. "a suite was created in Suite Manager") flows through Tauri events.

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

### Apply workspace patch flow

```
User picks workspace dir + suite + tool + clicks Apply to Workspace
  -> UI confirms with workspace dir + suite name
  -> UI invokes cmd_apply_workspace_patch({ workspaceDir, toolId, suiteId })
       -> workspace_patch::apply():
            1. validate workspaceDir exists and is a directory
            2. resolve realpath(workspaceDir) as boundary
            3. build workspace-scoped adapter (different target paths than global)
            4. read prior manifest if any
            5. for each prior path:
                 sentinel "::managed-section" -> rule_sync with empty enabled-list
                 regular path -> fs::remove + prune empty parent dirs up to ws root
            6. for each suite item:
                 hard-copy skill dir / agent file / cursor rule file
                 dereference any symlinks during copy
            7. for Codex/Claude rules: rule_sync with managed section
            8. build new manifest, write atomically (.tmp + rename)
       -> returns WorkspacePatchResult
```

The manifest is the entire memory of what was written. Re-apply with a different suite or tool reads the manifest, cleans the prior payload (including sentinel-driven managed-block cleanup), then writes the new payload. No drift detection in v1.

## Data model

### CapabilityItem

```rust
pub enum CapabilityKind { Skill, Agent, Rule }

pub struct CapabilityItem {
    pub id: String,                  // e.g. "skill:dev/repo-research"
    pub kind: CapabilityKind,
    pub name: String,                // basename or skill folder name
    pub source_path: PathBuf,        // absolute path under shared root
    pub relative_path: PathBuf,      // path relative to <shared>/<kind>/
    pub valid: bool,
    pub validation_errors: Vec<String>,
}
```

### ToolAdapter

```rust
pub struct ToolAdapter {
    pub id: ToolId,                          // Codex | Claude | Cursor | OpenClaw
    pub enabled: bool,
    pub skills_path: PathBuf,
    pub agents_path: PathBuf,
    pub rules_path: PathBuf,
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

### WorkspacePatchManifest

```rust
pub struct WorkspacePatchManifest {
    pub version: u32,                  // 1
    pub applied_at: String,            // ISO 8601
    pub tool: ToolId,
    pub suite: SuiteRef,
    pub paths: Vec<String>,            // workspace-relative, may include "<file>::managed-section" sentinel
}
```

Persisted at `<ws>/.agentic-hub/workspace-patch.json`.

## Security model

Trust boundary: the WebView is **untrusted**. The Rust core is **trusted**.

- All filesystem mutations happen in `agentic-core`, never in the WebView
- Tauri capability files scope FS access to a known allowlist; the dialog plugin adds runtime scope for user-picked workspace dirs only
- `shell:execute` is never granted to the WebView
- All target paths normalized to absolute paths via `path.canonicalize()` before validation
- Workspace patch enforces `path.starts_with(real_workspace_dir)` for every target
- Symlinks inside the shared root are followed for validation, but never followed across the workspace boundary during workspace patch copy (always dereferenced and copied as their resolved content)
- No remote network calls; no telemetry; no auto-update endpoint pinging in v1

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
| `workspace_patch` | Workspace dir does not exist | Apply blocked | Surface error before any FS write |
| `workspace_patch` | Out-of-workspace target resolved | Per-path error | Skip that path; record in result; continue |
| `workspace_patch` | Crash mid-apply | Atomic manifest write protects state | Next apply reads partially-written or last-known manifest; cleanup pass is idempotent |
| Tauri capability denial | App tries to read outside scope | Op fails at IPC layer | Bubble error to UI; user can adjust scope in settings or pick a different workspace |

No failure mode in v1 is silent.

## What's intentionally not here

- **Background filesystem watchers.** Scan on open + manual refresh only. Watchers add complexity, race conditions, and battery cost for a use case where the user explicitly knows when they changed something.
- **Cloud sync of suites.** Personal tool, single machine. Multi-machine sync is a follow-on.
- **Team capability libraries.** This is a single-user app. Team sharing is a different product.
- **Auto-detection of installed tools.** Settings are explicit. Auto-detection adds magic that breaks when a tool path changes.
- **Workspace target for OpenClaw.** Out of scope per the VS Code feature spec; revisit once OpenClaw's project-level scan path stabilizes.
- **Rich preview of skill / agent content.** This is a manager, not an editor. Users edit content in their existing editor.
- **Drift detection.** No watcher means no drift detection. Workspace manifest is updated only by apply.
- **Auto-update.** `tauri-plugin-updater` is supported but requires a signed manifest endpoint. Deferred until v1 public release.
- **Telemetry.** None. Personal-tool app, single user.
- **E-Studio remote sync.** Belongs in `e-studio-copilot`. Not part of this product.

## Open questions

- **Cross-window event channel granularity.** When Suite Manager creates / edits / deletes a suite, the main window's suite dropdown must refresh. Two options: (1) Tauri global event broadcast (`emit_all("suite-store-changed")`); (2) the main window polls on focus. Default for v1: global event broadcast. Validate during M2.
- **Type codegen toolchain.** `ts-rs` is the boring default. `specta` is newer and arguably more ergonomic. Decide at M0 scaffolding time; either is reversible.
- **macOS code-signing.** Required for Gatekeeper acceptance without user override. Decide before M4 launch hardening.
- **`tauri-plugin-store` vs custom JSON.** The store plugin gives us atomic writes for free for small state. We may use it for `state.json` (workspace target store) and keep `config.json` + `agentic-suites.json` + `workspace-patch.json` as custom JSON because their schemas are stable and we want migration control. Decide at M0.
