# Architecture: Workspace Inventory

This document deepens the workspace-scope design for Agentic Hub. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for the full system view.

## Context from root architecture

The root architecture establishes two scopes. **Global** scope *writes* into tool homes under the user's home directory using symlinks and managed copies (see [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md)). **Workspace** scope is a **read-only audit**: it never writes. It walks a remembered project directory's own per-tool folders and reports which agentic resources each tool (Codex / Claude / Cursor) already has installed there.

> History: workspace scope previously *hard-copied a suite into a project* (a "workspace patch" with a manifest and clean-then-write cycle). That write flow was removed; workspace scope is now inventory-only. The shared root and global projection remain the single place capabilities are written.

## Why this domain is split out

Workspace scope reads a fundamentally different filesystem shape than global scope:

- **Per-tool project dirs, not tool homes.** A workspace stores its resources under `.cursor/`, `.claude/`, `.agents/`, plus `AGENTS.md` / `CLAUDE.md` at the repo root — not under `~`.
- **Read-only.** There is no plan, no apply, no manifest, no cleanup. The scan produces rows; the UI renders them static.
- **Inverse of projection.** Global projection asks "given the shared root, what should each tool home contain?". Workspace inventory asks "given a project, what does each tool already have?".

## Goals

- Define the workspace target store and LRU semantics (unchanged).
- Define the per-tool workspace directories the scanner reads.
- Define the inventory scan: discovery, cross-tool dedupe, and the resulting rows/states.
- Define how the watcher live-refreshes the inventory.

## Non-goals

- Any workspace write (suite apply, hard copy, manifest). Removed.
- Per-item enable/disable in workspace scope. The view is read-only.
- OpenClaw workspace inventory (its workspace adapter is disabled).
- Hooks and managed-block *parsing* in the inventory (deferred; see follow-ups).
- Config-driven custom workspace paths (hard-coded per tool).

## Scope and boundaries

In scope:
- `workspace_inventory` module in `agentic-core` (the read-only scanner).
- `workspace_target_store` (LRU of remembered workspace dirs).
- Workspace-scoped adapter paths for Codex / Claude / Cursor (`create_workspace_adapter`).
- Watcher subscription to the active workspace's tool dirs.

Out of scope:
- Global-scope projection (see [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md)).
- Tauri capability layer (see [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md)).
- UI presentation (see [docs/features/workspace-inventory.md](docs/features/workspace-inventory.md)).

## Trust model

```
+--------------------+            +---------------------------+
|  WebView           |  IPC only  |  agentic-core             |
|  picks workspace   | ---------> |  workspace_target_store   |
|  via dialog        |            |  - validates dir exists   |
|                    |            |  - canonicalizes path     |
|                    |            |  - stores in LRU          |
+--------------------+            +---------------------------+
                                                |
                                                v
                                   +---------------------------+
                                   |  workspace_inventory      |
                                   |  scan_workspace(ws, tools)|
                                   |  - read-only walk         |
                                   |  - never writes           |
                                   +---------------------------+
```

The scan is read-only, so the out-of-workspace *write* guard is moot — there is nothing to guard. Path canonicalization still happens at pick time in the target store.

## Workspace target store

Persisted in `~/.agentic-hub/state.json`:

```json
{
  "workspaceTargets": [
    { "id": "ws-001", "label": "foo", "dir": "/Users/arno/Code/foo", "lastUsedAt": "2026-06-04T..." }
  ],
  "workspaceActiveId": "ws-001"
}
```

`add` canonicalizes and upserts by canonical path, bumps `lastUsedAt`, sets the entry active, and caps the list at 12 (LRU eviction). `remove` / `set_active` / `get_active` round out the API. The `last_applied` field and `record_apply` method from the patch era are gone.

## Per-tool workspace directories

`adapter_registry::create_workspace_adapter(tool, ws)` returns the per-tool paths the scanner reads:

| Tool | `skills_path` | `agents_path` | `rules_path` (scanned) | `instructions_path` |
|------|---------------|---------------|------------------------|---------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.agents/agents` | — | `<ws>/AGENTS.md` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` | — | `<ws>/CLAUDE.md` |
| Cursor | `<ws>/.cursor/skills` | `<ws>/.cursor/agents` | `<ws>/.cursor/rules` | — |
| OpenClaw | (adapter disabled in workspace scope) | | | |

The scanner reads `rules_path` only for Cursor (per-file `.cursor/rules`). Codex/Claude keep their rules in the managed block inside `AGENTS.md` / `CLAUDE.md`; the inventory surfaces the presence of those instruction files as a single `rule` row (`rule:AGENTS.md` / `rule:CLAUDE.md`) rather than parsing the block (deferred).

## Inventory scan

`workspace_inventory::scan_workspace(ws, tools) -> WorkspaceInventory { items, states, errors }`:

```
for each tool in tools (skip if adapter disabled):
    skills_path:  every dir containing SKILL.md  -> skill:<rel>
    agents_path:  every *.md                     -> agent:<rel>
    rules_path (Cursor only): every *.md / *.mdc -> rule:<rel>
    instructions_path (if file exists):          -> rule:<AGENTS.md|CLAUDE.md>

    for each discovered resource:
        push ToolCapabilityState { tool, item_id, state: Enabled, target_path }
        first tool to own an id contributes the displayed CapabilityItem
```

- **Dedupe by id across tools.** The same skill present in `.cursor/skills/x` and `.claude/skills/x` is one row with two `Enabled` states (two checked cells).
- **Only present resources are emitted**, so every state is `Enabled` and the matrix shows only enabled rows — there is nothing absent to toggle.
- `source_id` / `source_label` are the synthetic `"workspace"` / `"Workspace"`.
- Reuses the shared scanner's conventions: `__archived__` is skipped, walks are depth-bounded, missing tool dirs are not errors.

The result reuses the existing `CapabilityItem` / `ToolCapabilityState` / `ScanError` types, so the UI renders it through the same matrix as a global scan + inspect.

## Watcher integration

The watcher subscribes to the configured shared source roots (global) **and** the active workspace's existing tool dirs (`.agents`, `.claude`, `.cursor`, `.codex` recursive) plus `AGENTS.md` / `CLAUDE.md`. A change in either tree triggers a debounced pass that:

- reconciles the global projections (unchanged), then
- emits both `sources-changed` (global matrix) and `workspace-changed` (read-only inventory).

The UI reloads the active workspace inventory on `workspace-changed` only while in workspace scope. Picking, activating, or removing a workspace restarts the watcher so it re-subscribes to the new active dirs.

## Components and responsibilities

| Component | Responsibility |
|-----------|----------------|
| `workspace_target_store` | LRU persistence of remembered workspace dirs; canonicalize on add |
| `adapter_registry::create_workspace_adapter` | Per-tool workspace directory map |
| `workspace_inventory::scan_workspace` | Read-only walk → rows + present states |
| `watcher` (`agentic-hub`) | Subscribe to active workspace tool dirs; emit `workspace-changed` |

## Key flows

### Scan a workspace

```
UI -> cmd_scan_workspace({ workspace_id })
  agentic-hub bin -> resolve dir from workspace_target_store
                  -> workspace_inventory::scan_workspace(dir, WORKSPACE_TOOL_IDS)
                  -> return WorkspaceInventory
UI -> manager.loadWorkspace builds the read-only matrix
```

### Pick / activate / remove a workspace

```
UI -> cmd_pick_workspace_dir | cmd_set_active_workspace_target | cmd_remove_workspace_target
  agentic-hub bin -> mutate workspace_target_store
                  -> watcher.restart_if_running(app)   // re-subscribe to new active dirs
UI -> workspace store reloads targets; manager.loadWorkspace(activeId)
```

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| `cmd_pick_workspace_dir` | User cancels dialog | No-op | UI unchanged |
| `cmd_pick_workspace_dir` | Picked dir missing (race) | Refused | Surface error; LRU not updated |
| `cmd_scan_workspace` | Workspace id unknown | Refused | Surface error |
| `scan_workspace` | A tool dir is absent | Treated as empty | No error row |
| `scan_workspace` | A tool dir is unreadable | `ScanError` row | Other tools still scanned |
| `workspace_target_store` | `state.json` write fails | LRU lost across restarts | Surface; in-memory state preserved |

## Operational rules

- Workspace scope never writes. If a feature needs to write into a project, that is global projection, not workspace inventory.
- The scan is the source of truth for the view; there is no manifest.
- Workspace tool paths are hard-coded per tool.

## Related detailed docs

- [docs/features/workspace-inventory.md](docs/features/workspace-inventory.md) — user-facing feature spec
- [docs/tech/modules/workspace-inventory.md](docs/tech/modules/workspace-inventory.md) — scanner implementation details
- [docs/tech/modules/multi-source-roots.md](docs/tech/modules/multi-source-roots.md) — shared-root scanner this mirrors
- [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md) — `cmd_scan_workspace` and workspace events

## Follow-ups

- **Managed-block parsing.** Surface individual Codex/Claude rules inside `AGENTS.md` / `CLAUDE.md` instead of one presence row.
- **Hooks inventory.** Read `.cursor/hooks.json`, `.codex/hooks.json`, `.claude/settings.json` for installed hooks.
- **Workspace-scope OpenClaw.** Enable once OpenClaw has a stable project-level layout.
