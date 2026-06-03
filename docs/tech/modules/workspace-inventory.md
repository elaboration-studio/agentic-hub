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

| Tool | `skills_path` | `agents_path` | `rules_path` (scanned) | `instructions_path` |
|------|---------------|---------------|------------------------|---------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.agents/agents` | — | `<ws>/AGENTS.md` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` | — | `<ws>/CLAUDE.md` |
| Cursor | `<ws>/.cursor/skills` | `<ws>/.cursor/agents` | `<ws>/.cursor/rules` | — |

Codex projects under `.agents/` (its documented project skill path), not `.codex/`.

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
skills_path:  walk; every dir with SKILL.md  -> id = skill:<rel>,  name = folder
agents_path:  walk; every *.md               -> id = agent:<rel>,  name = file stem
rules_path (Cursor only): walk; *.md / *.mdc -> id = rule:<rel>,   name = file stem
instructions_path (if it is a file):          -> id = rule:<file>,  name = file name
```

Rules:

- IDs use the shared `kind:relpath` form (`skill:dev/tdd`, `rule:precise.mdc`, `rule:AGENTS.md`). Paths are unix-normalized.
- **Dedupe by id across tools.** The first tool that owns an id contributes the displayed `CapabilityItem` (its `source_path` / `relative_path`); every tool that has it emits a `ToolCapabilityState`.
- Every emitted state is `Enabled` (only present resources are scanned), so the matrix renders only enabled rows.
- `source_id` = `"workspace"`, `source_label` = `"Workspace"`.
- `__archived__` dirs are skipped; walks are depth-bounded (16) like the shared scanner; a missing tool dir is not an error; an unreadable dir produces a `ScanError`.

## Watcher integration

The `agentic-hub` watcher subscribes to the shared source roots **and** the active workspace's existing tool dirs (`.agents`, `.claude`, `.cursor`, `.codex` recursive) plus `AGENTS.md` / `CLAUDE.md`. A debounced batch reconciles global projections, then emits both `sources-changed` and `workspace-changed`. Picking / activating / removing a workspace calls `restart_if_running` so the watcher re-subscribes to the new active dirs.

## IPC surface

- `cmd_scan_workspace(workspace_id) -> WorkspaceInventory` — resolve the dir from the target store, scan it.
- `cmd_pick_workspace_dir`, `cmd_list_workspace_targets`, `cmd_set_active_workspace_target`, `cmd_remove_workspace_target` — target store CRUD; the mutating ones restart the watcher.
- Event `workspace-changed` — the UI reloads the active inventory while in workspace scope.

See [tauri-ipc-contract.md](./tauri-ipc-contract.md).

## Testing

TDD unit tests in `workspace_inventory.rs`: per-tool discovery, cross-tool dedupe (one row, two present states), instruction-file presence, empty workspace → nothing, `__archived__` skipped, OpenClaw skipped. UI: `manager.loadWorkspace` populates a read-only inventory with no pending keys and refuses toggles; `refresh` clears `readOnly`; the workspace store reload/pick/activate/remove handoff to `loadWorkspace`.

## Follow-ups

- Parse the Codex/Claude managed block into individual rule rows.
- Inventory installed hooks.
- Enable OpenClaw workspace inventory once it has a stable project layout.
