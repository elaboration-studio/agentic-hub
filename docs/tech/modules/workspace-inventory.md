# Module: Workspace Inventory

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.workspace.md](../../../ARCHITECTURE.workspace.md), [docs/tech/modules/multi-source-roots.md](./multi-source-roots.md)
Related Docs: [docs/features/workspace-inventory.md](../../features/workspace-inventory.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Specify the read-only workspace inventory: how `agentic-core` walks a project's own per-tool directories and reports which agentic resources each tool already has. Workspace scope performs **no filesystem writes**; this module is the inverse of global projection.

> The previous "workspace patch" module (hard-copy a suite into a project, with a manifest and clean-then-write cycle) was removed. The shared root + global projection remain the only place capabilities are written.

## Scope model

- `SyncScope = Global | Workspace`. The main window has a header toggle.
- `Global` keeps the per-tool home-directory projection (symlinks + managed copies).
- `Workspace` reads a remembered project directory and reports its inventory.
- Workspace scope covers `Codex`, `Claude`, `Cursor` (`WORKSPACE_TOOL_IDS`); OpenClaw's workspace adapter is disabled.

## Workspace target store

Persisted at `~/.agentic-hub/state.json`:

```json
{
  "workspaceTargets": [
    { "id": "ws-001", "label": "foo", "dir": "/Users/arno/Code/foo", "lastUsedAt": "2026-06-04T01:00:00Z" }
  ],
  "workspaceActiveId": "ws-001"
}
```

`add` canonicalizes and upserts by canonical path, bumps `lastUsedAt`, sets the entry active, caps at 12 (LRU). `remove` / `set_active` / `get_active` / `read` complete the API. There is no `last_applied` / `record_apply` (removed with the patch flow).

## Per-tool directories read

`adapter_registry::create_workspace_adapter(tool, ws)` provides the paths:

| Tool | skill dirs | agent dirs (ext) | `rules_path` (scanned) | `instructions_path` |
|------|------------|------------------|------------------------|---------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.codex/agents` (`*.toml`) | — | `<ws>/AGENTS.md` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` (`*.md`) | — | `<ws>/CLAUDE.md` |
| Cursor | `<ws>/.cursor/skills` **+ `<ws>/.agents/skills`** | `<ws>/.cursor/agents` **+ `<ws>/.agents/agents`** (`*.md`) | `<ws>/.cursor/rules` | `<ws>/AGENTS.md` |

Verified against the 2026 tool docs:

- **Codex skills** live under `.agents/skills` (its documented project skill path), but **Codex subagents are TOML files under `.codex/agents`** — `.agents/` is skills-only ([Codex subagents](https://developers.openai.com/codex/subagents)). The scanner matches `*.toml` for Codex agents, `*.md` otherwise.
- **`AGENTS.md` is read by both Codex and Cursor**, so both adapters set `instructions_path` to it and the inventory attributes that row to both tools ([Cursor rules](https://cursor.com/docs/rules)). `CLAUDE.md` is Claude-only.
- **Cursor honors the shared `.agents/` standard dir** (skills) in addition to its own `.cursor/` dirs, so a skill dropped in `.agents/skills` surfaces for both Codex and Cursor ([Cursor skills](https://cursor.com/docs/skills)).

## Scan contract

```rust
pub fn scan_workspace(ws: &Path, tools: &[ToolId]) -> WorkspaceInventory;

pub struct WorkspaceInventory {
    pub items: Vec<CapabilityItem>,        // one row per distinct id, sorted by id
    pub states: Vec<ToolCapabilityState>,  // one per (tool, present item), always Enabled
    pub errors: Vec<ScanError>,
}
```

Algorithm per tool (skipped when the adapter is disabled):

```
skill dirs:  walk; every dir with SKILL.md          -> id = skill:<rel>,  name = folder
agent dirs:  walk; Codex *.toml else *.md            -> id = agent:<rel>,  name = file stem
rules_path (Cursor only): walk; *.md / *.mdc         -> id = rule:<rel>,   name = file stem
instructions_path (if it is a file):                 -> id = rule:<file>,  name = file name
```

Rules:

- IDs use the shared `kind:relpath` form (`skill:dev/tdd`, `rule:precise.mdc`, `rule:AGENTS.md`). Paths are unix-normalized.
- **Dedupe by id across tools.** The first tool that owns an id contributes the displayed `CapabilityItem` (its `source_path` / `relative_path`); every tool that has it emits a `ToolCapabilityState`.
- **Dedupe states per tool.** A resource reachable from more than one of a tool's dirs (e.g. Cursor reading both `.cursor/skills` and `.agents/skills`) emits exactly one state for that tool.
- Every emitted state is `Enabled` (only present resources are scanned), so the matrix renders only enabled rows.
- `source_id` = `"workspace"`, `source_label` = `"Workspace"`.
- `__archived__` dirs are skipped; walks are depth-bounded (16) like the shared scanner; a missing tool dir is not an error; an unreadable dir produces a `ScanError`.

## Global × local merge (UI store)

`scan_workspace` reports only the project's **local** resources. The full "what applies to this project?" view is assembled in the UI store (`manager.loadWorkspace`), which merges two sources before handing the matrix a single read-only dataset:

- **Global (applied):** `scan(sources)` + `inspect` over the shared roots, filtered to states that are `Enabled` **and** belong to a workspace tool. A globally-enabled resource is projected into the tool's home dir, so it applies to every project. Items with no surviving state are dropped.
- **Local:** `scan_workspace` output. Local item ids and state `itemId`s are namespaced with a `ws::` prefix so a global and a local resource sharing the same relative path stay **distinct rows**, each carrying its own source (`.agentic-arno`, `.helper`, … vs `Workspace`).

Both sets are unioned into `items` / `states`. Tool columns are fixed to `WORKSPACE_TOOLS` (Codex / Claude / Cursor). The matrix's source filter appears automatically (more than one source) and lets the user narrow to a single shared root or to `Workspace`. The `ws::` prefix uses the same `::` delimiter as `key()`, so tool parsing is unaffected. A `sources-changed` event reloads the merged inventory while in workspace scope (globals can change too), alongside `workspace-changed` for local edits.

## Watcher integration

The `agentic-hub` watcher subscribes to the shared source roots **and** the active workspace's existing tool dirs (`.agents`, `.claude`, `.cursor`, `.codex` recursive) plus `AGENTS.md` / `CLAUDE.md`. A debounced batch reconciles global projections, then emits both `sources-changed` and `workspace-changed`. Picking / activating / removing a workspace calls `restart_if_running` so the watcher re-subscribes to the new active dirs.

## IPC surface

- `cmd_scan_workspace(workspace_id) -> WorkspaceInventory` — resolve the dir from the target store, scan it.
- `cmd_pick_workspace_dir`, `cmd_list_workspace_targets`, `cmd_set_active_workspace_target`, `cmd_remove_workspace_target` — target store CRUD; the mutating ones restart the watcher.
- Event `workspace-changed` — the UI reloads the active inventory while in workspace scope.

See [tauri-ipc-contract.md](./tauri-ipc-contract.md).

## Testing

TDD unit tests in `workspace_inventory.rs`: per-tool discovery, cross-tool dedupe (one row, two present states), Cursor reading the shared `.agents/` dir, per-tool state dedupe across `.cursor`/`.agents`, instruction-file presence (`AGENTS.md` attributed to both Codex **and** Cursor; `CLAUDE.md` to Claude only), Codex subagents read from `.codex/agents/*.toml` (markdown in `.agents/agents` is not a Codex subagent), Cursor recursive nested skill discovery (name = leaf folder), Cursor nested `.mdc` rule discovery, empty workspace → nothing, `__archived__` skipped, OpenClaw skipped. Adapter-layout truths live in `adapter_registry.rs` (Claude skills Flat, Claude agents Nested + recursive). UI: `manager.loadWorkspace` populates a read-only inventory with no pending keys and refuses toggles, merges only `Enabled` global states with namespaced local resources, and `refresh` clears `readOnly`; the workspace store reload/pick/activate/remove handoff to `loadWorkspace`.

## Follow-ups

- Parse the Codex/Claude managed block into individual rule rows.
- Inventory installed hooks.
- Enable OpenClaw workspace inventory once it has a stable project layout.
