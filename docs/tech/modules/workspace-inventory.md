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
- Workspace scope covers `Codex`, `Claude`, `Cursor`, `Kiro`, `Copilot`, and
  `Antigravity` (`WORKSPACE_TOOL_IDS`); OpenClaw and OpenStandard remain
  global-only.

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

| Tool | skill dirs | agent dirs (ext) | `rules_path` (scanned) | hooks | `instructions_path` |
|------|------------|------------------|------------------------|-------|---------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.codex/agents` (`*.toml`) | — | `<ws>/.codex/hooks.json` | `<ws>/AGENTS.md` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` (`*.md`) | — | (in `settings.json`) | `<ws>/CLAUDE.md` |
| Cursor | `<ws>/.cursor/skills` **+ `<ws>/.agents/skills`** | `<ws>/.cursor/agents` **+ `<ws>/.agents/agents`** (`*.md`) | `<ws>/.cursor/rules` | `<ws>/.cursor/hooks.json` | `<ws>/AGENTS.md` |
| Kiro | `<ws>/.kiro/skills` | `<ws>/.kiro/agents` (`*.md`) | `<ws>/.kiro/steering` | `<ws>/.kiro/hooks/*.json` | — |
| Copilot | `<ws>/.github/skills` **+ `<ws>/.agents/skills`** | `<ws>/.github/agents` (`*.agent.md`) | `<ws>/.github/instructions` (`*.instructions.md`) | `<ws>/.github/hooks/*.json` | `<ws>/.github/copilot-instructions.md` |
| Antigravity | `<ws>/.agents/skills` **+ `<ws>/.agent/skills`** | — | `<ws>/.agents/rules` **+ `<ws>/.agent/rules`** | aggregate file, not inventoried | `<ws>/AGENTS.md` |

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
    pub locked_skills: Vec<LockedSkill>,   // skill items the skills.sh CLI manages
}

pub struct LockedSkill { pub item_id: String, pub name: String,
                         pub source: String, pub source_type: String }
```

Algorithm per tool (skipped when the adapter is disabled):

```
skill dirs:  walk; every dir with SKILL.md          -> id = skill:<rel>,  name = folder
agent dirs:  walk; Codex *.toml else *.md            -> id = agent:<rel>,  name = file stem
rules_path (tool-specific): walk supported rule files -> id = rule:<rel>, name = file stem
per-hook dirs (Kiro/Copilot): every *.json            -> id = hook:<rel>, name = file stem
instructions_path (if it is a file):                 -> id = rule:<file>,  name = file name
```

Rules:

- IDs use the shared `kind:relpath` form (`skill:dev/tdd`, `rule:precise.mdc`, `rule:AGENTS.md`). Paths are unix-normalized.
- **Dedupe by id across tools.** The first tool that owns an id contributes the displayed `CapabilityItem` (its `source_path` / `relative_path`); every tool that has it emits a `ToolCapabilityState`.
- **Dedupe states per tool.** A resource reachable from more than one of a tool's dirs (e.g. Cursor reading both `.cursor/skills` and `.agents/skills`) emits exactly one state for that tool.
- Every emitted state is `Enabled` (only present resources are scanned), so the matrix renders only enabled rows.
- `source_id` = `"workspace"`, `source_label` = `"Workspace"`.
- `__archived__` dirs are skipped; walks are depth-bounded (16) like the shared scanner; a missing tool dir is not an error; an unreadable dir produces a `ScanError`.

## skills.sh lock marking

After building items, the scan reads the project lock via
`skill_lock::read_local_lock(ws)` (see [skill-sources.md](./skill-sources.md))
and, for each `Skill` item whose **leaf folder name** is a key in the lock's
`skills` map, emits a `LockedSkill { item_id, name, source, source_type }`
(`item_id` is the local `skill:<rel>` id, pre-namespacing). This is how the UI
marks a row as skills.sh-managed and targets `npx skills update`. Marking is
tolerant — no lock file (or a malformed one) means an empty `locked_skills`, so a
third-party lock never breaks the read-only scan. The UI store namespaces each
`item_id` with the `ws::` prefix so the map key matches the matrix row id.

## Global × local merge (UI store)

`scan_workspace` reports only the project's **local** resources. The full "what applies to this project?" view is assembled in the UI store (`manager.loadWorkspace`), which merges two sources before handing the matrix a single read-only dataset:

- **Global (applied):** `scan(sources)` + `inspect` over the shared roots, filtered to states that are `Enabled` **and** belong to a workspace tool. A globally-enabled resource is projected into the tool's home dir, so it applies to every project. Items with no surviving state are dropped.
- **Local:** `scan_workspace` output. Local item ids and state `itemId`s are namespaced with a `ws::` prefix so a global and a local resource sharing the same relative path stay **distinct rows**, each carrying its own source (`.agentic-arno`, `.helper`, … vs `Workspace`).

Both sets are unioned into `items` / `states`. Tool columns are fixed to
`WORKSPACE_TOOLS` (Codex / Claude / Cursor / Kiro / Copilot / Antigravity).
The matrix's source filter appears automatically (more than one source) and
lets the user narrow to a single shared root or to `Workspace`. The `ws::`
prefix uses the same `::` delimiter as `key()`, so tool parsing is unaffected.
A `sources-changed` event reloads the merged inventory while in workspace scope
(globals can change too), alongside `workspace-changed` for local edits.

The store also builds `lockedSkills: Map<namespacedItemId, { name, source }>` from `inv.lockedSkills` (applying the `ws::` prefix), which the matrix reads to render a `skills.sh` badge and the row's **Update via skills.sh** action. It is reset to empty in global scope (`refresh`).

## Watcher integration

The `agentic-hub` watcher subscribes to the shared source roots **and** the
active workspace's existing tool dirs (`.agents`, `.agent`, `.claude`,
`.cursor`, `.codex`, `.kiro`, `.github` recursively) plus `AGENTS.md` /
`CLAUDE.md`. A debounced batch reconciles global projections, then emits both
`sources-changed` and `workspace-changed`. Picking / activating / removing a
workspace calls `restart_if_running` so the watcher re-subscribes to the new
active dirs.

Aggregate workspace hook files are not decomposed into inventory rows. This
currently excludes Codex, Claude, Cursor, and Antigravity hooks. Kiro and
Copilot hooks are inventoried because their one-file-per-hook layouts provide a
stable item identity without interpreting foreign aggregate content.

## IPC surface

- `cmd_scan_workspace(workspace_id) -> WorkspaceInventory` — resolve the dir from the target store, scan it.
- `cmd_pick_workspace_dir`, `cmd_list_workspace_targets`, `cmd_set_active_workspace_target`, `cmd_remove_workspace_target` — target store CRUD; the mutating ones restart the watcher.
- Event `workspace-changed` — the UI reloads the active inventory while in workspace scope.
- Skills.sh install + update (opt-in source only) — the **two** explicit workspace writes, both driven from the `install` window. They run a starred skill's install (`npx skills add`) or a locked skill's update (`npx skills update`) via a controlled subprocess, then emit `workspace-changed` so this read-only scan re-runs. The scan never writes; see [skill-sources.md](./skill-sources.md).

See [tauri-ipc-contract.md](./tauri-ipc-contract.md).

## Testing

TDD unit tests in `workspace_inventory.rs`: per-tool discovery, cross-tool dedupe (one row, two present states), Cursor reading the shared `.agents/` dir, per-tool state dedupe across `.cursor`/`.agents`, instruction-file presence (`AGENTS.md` attributed to both Codex **and** Cursor; `CLAUDE.md` to Claude only), Codex subagents read from `.codex/agents/*.toml` (markdown in `.agents/agents` is not a Codex subagent), Cursor recursive nested skill discovery (name = leaf folder), Cursor nested `.mdc` rule discovery, empty workspace → nothing, `__archived__` skipped, OpenClaw skipped, plus skills.sh lock marking (a skill in the lock is marked with its source; an unlocked skill is not; no lock file → empty). Adapter-layout truths live in `adapter_registry.rs` (Claude skills Flat, Claude agents Nested + recursive). UI: `manager.loadWorkspace` populates a read-only inventory with no pending keys and refuses toggles, merges only `Enabled` global states with namespaced local resources, populates namespaced `lockedSkills` from the inventory lock, and `refresh` clears `readOnly` and `lockedSkills`; the workspace store reload/pick/activate/remove handoff to `loadWorkspace`.

## Follow-ups

- Parse the Codex/Claude managed block into individual rule rows.
- Inventory installed hooks.
- Enable OpenClaw workspace inventory once it has a stable project layout.
