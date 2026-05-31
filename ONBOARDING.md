# Agentic Hub Onboarding Guide

## What Is This?

Agentic Hub is a Tauri 2.x desktop app that manages one shared tree of
agentic capabilities — skills, agents, rules, and hooks — and projects it into
the tools you actually use: Codex, Claude Code, Cursor, and OpenClaw. You keep
a single source of truth under `~/.agentic/`, then turn each capability on or
off per tool from one window instead of hand-copying files into four different
config directories.

The product is built around one safety principle: **the Rust core owns every
filesystem mutation, and the WebView is untrusted.** You stage changes in the
UI, the core plans them against fresh disk state, and nothing is written until
you explicitly apply. Real files you own are never overwritten — conflicts
surface as state, not silent clobbering.

Three things make it more than a file copier: **suites** (named capability
presets you apply in one click), **workspace patches** (hard-copy a suite into
a specific project folder), and a per-tool projection engine that picks the
right mechanism — symlink, managed copy, managed markdown block, or managed
JSON entry — for each tool and capability kind.

---

## How It's Used

Agentic Hub is an end-user desktop product. You launch the window and see a
matrix: capability rows (grouped into Skills, Agents, Rules, Hooks) against
tool columns (Codex, Claude, Cursor, OpenClaw). Each cell is a toggle showing
whether that capability is currently projected into that tool.

The primary workflows:

- **Manage capabilities** — toggle cells, then click Apply. A warning dot on a
  cell flags an abnormal state (broken link, stale copy, or a real file
  blocking the target).
- **Save and apply suites** — capture the currently enabled set for a tool as a
  named suite, then apply any suite to a tool in one click (a full reset:
  everything in the suite on, everything else off).
- **Patch a workspace** — switch the header toggle from Global to Workspace,
  add a project folder, and hard-copy a suite into it for a single project.

There are two scopes, switched in the header: **Global** (projects into your
home tool directories like `~/.cursor/`) and **Workspace** (projects into a
chosen project directory). The whole UI lives in `src/App.tsx`.

---

## How Is It Organized?

Agentic Hub is a three-layer system: a React UI, a thin Tauri command shell,
and a pure Rust domain core that does all the real work. The UI never touches
the filesystem directly — it sends typed IPC payloads to the shell, which
forwards them to the core.

```
        You (desktop window)
              |
              v
   +------------------------+
   |  React + Vite UI       |   src/App.tsx, src/ipc.ts
   |  (untrusted WebView)   |
   +-----------+------------+
               |
               |  Tauri IPC (invoke "cmd_*")
               v
   +------------------------+
   |  Tauri shell           |   crates/agentic-hub/
   |  cmd_* wrappers        |   lib.rs, commands.rs, error.rs
   +-----------+------------+
               |
               |  plain Rust calls (no Tauri types)
               v
   +------------------------+
   |  agentic-core          |   crates/agentic-core/
   |  projection engine     |   scan / plan / apply / sync
   +-----------+------------+
               |
               |  filesystem mutations
               v
   +------------------------+
   |  Shared root + tool    |   ~/.agentic/, ~/.codex/, ~/.claude/,
   |  homes + workspaces    |   ~/.cursor/, ~/.openclaw/, <ws>/
   +------------------------+
```

The core is split into focused modules, each owning one concern:

| Module | Responsibility |
|--------|----------------|
| `scanner` | Discover capabilities across the source forest |
| `settings` | Load/save config; resolve the source roots |
| `model` | Shared domain types (sent to the UI via `ts-rs`) |
| `adapter_registry` | Per-tool, per-kind projection decisions + paths |
| `planner` | Inspect current disk state; diff into operations |
| `applier` | Execute planned operations on disk |
| `managed_copy` | Hard-copy files/folders + per-root manifest |
| `rule_sync` | Managed markdown block in instruction files |
| `hook_sync` | Managed JSON entries in tool hook configs |
| `suite_store` | CRUD over `~/.agentic-suites.json` |
| `workspace_patch` | Hard-copy a suite into a project folder |
| `workspace_target_store` | Track active workspace folders (LRU) |
| `api` | High-level orchestration the shell wraps 1:1 |
| `paths` | Home dir, `~` expand/tildify helpers |
| `error` | `thiserror` error enums |

Internal layout (only the parts that matter):

```
agentic-hub/
  src/                   # React + Vite UI
    App.tsx              # The entire UI (matrix, suites, ws)
    ipc.ts               # Typed wrappers over cmd_* handlers
    types/generated/     # ts-rs output (never hand-edit)
  crates/
    agentic-core/src/    # Pure domain engine (no Tauri)
    agentic-hub/src/     # Tauri shell: cmd_* + error mapping
  docs/                  # Product, architecture, module docs
  ARCHITECTURE*.md       # Root system-design specs
  PRODUCT.md             # Product requirements
```

**External dependencies and integrations.** Agentic Hub talks to no network
services. Its only "external" surface is the local filesystem plus two Tauri
plugins. The `shell` plugin is deliberately never loaded.

| Dependency | What it's used for | Configured via |
|-----------|--------------------|----------------|
| Local filesystem | Source of truth + all projection targets | Paths in settings |
| `tauri-plugin-dialog` | Native folder picker (workspace scope) | `cmd_pick_workspace_dir` |
| `tauri-plugin-store` | Frontend-side persisted store | Tauri plugin |
| `ts-rs` | Generate TS types from Rust | `gen:types` script |

The contract is filesystem state. There is no database and no shadow cache —
re-scanning disk is always the way to learn current state.

---

## Key Concepts and Abstractions

| Concept | What it means in this codebase |
|---------|--------------------------------|
| Capability | A unit you project: a `skill`, `agent`, `rule`, or `hook` (`CapabilityKind`) |
| Shared root | The source tree (default `~/.agentic/`) scanned for capabilities |
| Source forest | Multiple prioritized roots; first source wins on collisions |
| Adapter | Per-tool config mapping a kind to a target path + projection mode |
| Projection mode | How a kind is written: `LinkSync`, `FileSync`, `MarkdownSectionSync`, `JsonSection` |
| `LinkState` | Current target state: `enabled`/`disabled`/`broken`/`stale`/`foreign_file`/`foreign_link` |
| Desired map | UI staging: `tool::itemId -> bool` of what you want enabled |
| `PlannedOperation` | One planned disk mutation (carries its `targetRoot`) |
| Plan-then-apply | Stage in memory, plan from fresh disk, apply explicitly |
| Managed copy | A real file/folder recorded in a per-root `.agentic-hub-managed.json` manifest |
| Managed block | A markdown section between `<!-- agentic-hub:start -->` / `:end` markers |
| Suite | A named capability preset stored in `~/.agentic-suites.json` |
| Workspace patch | A suite hard-copied into `<ws>/.agentic-hub/` for one project |
| Foreign | A target owned by something other than us — apply refuses to clobber it |

Two architectural rules shape almost every change:

- **The core owns mutations.** UI code in `src/` only calls `invoke("cmd_*")`
  through `src/ipc.ts`. All disk logic lives in `agentic-core`; the shell
  (`crates/agentic-hub/src/commands.rs`) is a thin, testable marshalling layer.
- **Types flow Rust → TS.** Shared types are defined once in
  `agentic-core::model` (and `api`) with `ts-rs`, then generated into
  `src/types/generated/`. Never hand-edit the generated files; run the
  `gen:types` script instead.

---

## Primary Flows

### Flow 1: Inspect and apply (the core loop)

This is what happens from app open through applying a toggle. Every step maps
to an IPC command in `src/ipc.ts` and a handler in `crates/agentic-hub`.

```
App mounts -> refresh()                    src/App.tsx
  |
  v
loadSettings()  -> cmd_load_settings       resolve sources
  |
  v
scan(sources)   -> cmd_scan                scanner walks the forest
  |
  v
inspect(items)  -> cmd_inspect             planner reads disk state
  |
  v
seedDesired()                              desired map = current state
  |
  v
user toggles cells (in-memory only)
  |
  v
Apply, per modified tool:
  plan()        -> cmd_plan                planner diffs -> operations
  apply(ops)    -> cmd_apply               applier mutates disk
  syncRules()   -> cmd_sync_rules          rule_sync rewrites md block
  syncHooks()   -> cmd_sync_hooks          hook_sync rewrites json
  |
  v
refresh()                                  re-scan, re-inspect
```

The planner decides the mechanism per item via `adapter_registry`: symlinks for
most skills/agents, managed copies for Cursor agents and Claude skills, a
markdown block for rules on Codex/Claude/OpenClaw, and JSON entries for hooks.

### Flow 2: Apply a suite (full reset)

From the Suites bar, picking a suite and a tool calls `applySuite()` ->
`cmd_apply_suite`. The core computes a desired state where every capability in
the suite is enabled and everything else is disabled, then runs the same
plan/apply/sync pipeline as Flow 1. An empty suite disables everything for the
tool (the UI confirms first).

### Flow 3: Patch a workspace

In Workspace scope (`WorkspacePanel` in `src/App.tsx`): `pickWorkspaceDir()` ->
`cmd_pick_workspace_dir` opens the native picker and registers the folder, then
`applyWorkspacePatch()` -> `cmd_apply_workspace_patch` hard-copies the chosen
suite into `<ws>/.agentic-hub/`, dereferencing symlinks and writing managed
markdown/JSON sections for rules and hooks. Prior payload is cleaned first.

---

## Developer Guide

### Setup

```bash
pnpm install
cargo fetch
```

Prerequisites: Rust stable (1.78+), Node 20.x, pnpm 9.x, and the Tauri 2.x CLI
(`cargo install tauri-cli --version "^2.0"`). See
[`docs/tech/development/getting-started.md`](docs/tech/development/getting-started.md)
for the full prerequisite matrix and platform notes.

### Running and testing

```bash
pnpm tauri dev           # Vite UI + Tauri shell, hot reload
pnpm build               # tsc --noEmit && vite build (UI only)
cargo test --workspace   # Rust unit + integration tests
cargo clippy --all-targets   # Rust lints
cargo fmt --all          # format Rust
```

The defined npm scripts are `dev`, `build`, `preview`, `tauri`, and
`gen:types`. Run `pnpm tauri dev` for the full app; `pnpm dev` runs only the
Vite UI without the shell.

### Type codegen (Rust → TS)

Whenever you change a shared type in `agentic-core`, regenerate the TS bindings:

```bash
pnpm gen:types
# = cargo test -p agentic-core --features ts-export
#   && cargo test -p agentic-hub  --features ts-export
```

This rewrites `src/types/generated/*.ts`. Commit the regenerated files.

### Common change patterns

- **Add an IPC command** — define typed input/output in `agentic-core` with
  `serde` + `ts_rs::TS`, implement the handler (usually in
  `agentic-core/src/api.rs`), add a `#[tauri::command]` wrapper in
  `crates/agentic-hub/src/commands.rs`, register it in the
  `invoke_handler![...]` list in `crates/agentic-hub/src/lib.rs`, add it to the
  Tauri capability allowlist (`crates/agentic-hub/capabilities/default.json`),
  run `pnpm gen:types`, then add a wrapper in `src/ipc.ts`.
- **Add a tool adapter** — extend `ToolId` in `agentic-core::model`, add
  defaults in `settings`, wire projection logic in `adapter_registry`, update
  [`docs/tech/reference/tool-adapter-matrix.md`](docs/tech/reference/tool-adapter-matrix.md),
  and add a column in `TOOLS` in `src/App.tsx`.
- **Change a projection mechanism** — start in `adapter_registry`
  (`projection_mode_for`), then update the matching engine module
  (`planner` + `applier`/`managed_copy`/`rule_sync`/`hook_sync`).

### Key files to start with

| Area | File | Why |
|------|------|-----|
| UI | `src/App.tsx` | The entire window: matrix, suites, workspace |
| IPC contract | `src/ipc.ts` | Every `cmd_*` call in one typed module |
| Shell entry | `crates/agentic-hub/src/lib.rs` | Command registration + plugins |
| Shell handlers | `crates/agentic-hub/src/commands.rs` | Thin wrappers over core |
| Core orchestration | `crates/agentic-core/src/api.rs` | scan/inspect/apply entry points |
| Core surface | `crates/agentic-core/src/lib.rs` | Module map + public exports |
| Projection rules | `crates/agentic-core/src/adapter_registry.rs` | Where each kind goes, per tool |
| Domain types | `crates/agentic-core/src/model.rs` | Types mirrored to TS |

### Practical tips

- Every module in `agentic-core` is testable against a tempdir with no Tauri
  dependency — write engine tests there, not in the shell. The shell layer is
  intentionally thin.
- Managed-copy metadata lives in a per-root `.agentic-hub-managed.json`
  manifest, and rule blocks use the `<!-- agentic-hub:start -->` / `:end`
  markers. These on-disk contracts match the VS Code extension for migration
  parity — do not rename them. See
  [`ARCHITECTURE.projection.md`](ARCHITECTURE.projection.md).
- `tauri-plugin-shell` is never added. If a feature seems to need it, raise a
  security review first (see [`AGENTS.md`](AGENTS.md) hard rules).
- The `README.md` still says "pre-implementation" — that is stale; the engine,
  shell, and UI described above are implemented. Trust the code and
  `ARCHITECTURE*.md` over the README status line.
