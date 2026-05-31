# Module: Source Watcher & Reconcile

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-31
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/source-watcher.md](../../features/source-watcher.md), [docs/tech/modules/multi-source-roots.md](./multi-source-roots.md), [docs/tech/modules/workspace-patch.md](./workspace-patch.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Split of responsibility

The watcher is two layers, mirroring the project's core/shell boundary:

- **`agentic-core::reconcile`** — pure decision logic + pipeline reuse. No Tauri,
  no OS watcher. Fully unit-testable against tempdirs.
- **`agentic-hub::watcher`** — the OS subscription, debounce, Tauri event bridge,
  and active-workspace re-patch. Holds a `WatcherState` in Tauri managed state.

## Core: `reconcile`

```rust
pub fn compute_reconcile_desired(
    items: &[CapabilityItem],
    states: &[ToolCapabilityState],
    prev_known_ids: &HashSet<String>,
) -> HashMap<String, bool>;

pub fn reconcile_tool(items, settings, tool, prev_known_ids) -> ReconcileToolOutcome;
pub fn reconcile_all(items, settings, prev_known_ids) -> Vec<ReconcileToolOutcome>;
```

`compute_reconcile_desired` is the heart and the unit-test surface:

- **Owned** projection states (`Enabled`, `Stale`, `Broken`) stay enabled so the
  plan refreshes stale copies and repairs broken links. `Disabled` and the
  foreign states (`ForeignFile`, `ForeignLink`) stay off — the manager never
  takes over content it does not own.
- **Newcomers** (ids absent from `prev_known_ids`) auto-enable iff a
  same-`source_id`, same-kind sibling in the same parent folder is currently
  owned. Grouping key: `(source_id, kind-prefix, relative_path.parent())`. A
  newcomer in a brand-new folder has no enabled sibling, so it stays disabled.

`reconcile_tool` inspects current state (link/file/rule via `planner::inspect_tool`
plus hooks via `hook_sync::inspect_hooks`), builds the desired map, then reuses
the **existing pipeline** — `planner::build_plan` + `applier::apply` +
`api::sync_rules` + `api::sync_hooks`. There is no parallel engine.

`reconcile_all` loops `ToolId::ALL`, skipping tools whose adapter is disabled.

### Why `prev_known_ids` matters

The auto-enable rule fires only for ids the previous scan did not contain. The
hub seeds the snapshot to the current scan **at watcher start**, so launching the
app never auto-enables a flood. Only files that appear *after* the watcher is
running count as newcomers. The Config fallback passes the full current id set as
"known", giving a pure refresh/repair pass with no auto-enable.

## Hub: `watcher`

`WatcherState { inner: Mutex<Option<Running>> }` is `.manage()`d on the builder.

```
start(app):
  load settings; subscribe a notify::recommended_watcher over each source root
  (recursive). Seed prev = current scan ids. Spawn a worker thread.
stop():        drop the watcher (ends OS events) + signal the worker to exit.
set_enabled(): start or stop.
restart_if_running(): re-subscribe to current roots iff already running.
```

The OS callback forwards relevant events to the worker over an mpsc channel.
Events confined to a `.git/` path component are dropped (index/lock churn).

### Debounce + worker loop

```
loop:
  block for the first change
  drain the channel until quiet for DEBOUNCE (400 ms)   // coalesce a git pull
  run_once()
```

`run_once`: load settings → `api::scan` → `reconcile::reconcile_all(prev)` →
re-patch active workspace → update `prev` to the new ids → `emit("sources-changed")`.

`resync_now` (the fallback command) is the same minus auto-enable: it passes the
full current id set as `prev`, so nothing is treated as a newcomer.

### Active-workspace re-patch

Reads `WorkspaceTargetStore::get_active()`. For each `WorkspaceApply { tool_id,
suite_id }` in `lastApplied`, it replays `workspace_patch::apply_workspace_patch`
from fresh source content. `cmd_apply_workspace_patch` records `lastApplied` via
`WorkspaceTargetStore::record_apply` (upsert by tool).

## Loop avoidance

Target directories (`~/.claude`, `<ws>/.agentic-hub`, …) are never watched — only
source roots are. Reconcile is idempotent: an unchanged source yields an empty
plan and no writes. So even if a user points a source at a directory that
contains a target, the system converges in one or two cycles rather than looping.

## IPC + settings deltas

- `Settings.watcherEnabled: bool` (`#[serde(default = true)]`).
- `WorkspaceTarget.lastApplied: Vec<WorkspaceApply>` (`#[serde(default)]`).
- `cmd_set_watcher_enabled(enabled)` — persist + start/stop.
- `cmd_rescan_resync()` — full rescan + resync fallback.
- `cmd_save_settings` / `cmd_add_source` / `cmd_remove_source` call
  `restart_if_running` so watched roots track configuration.
- Event `sources-changed` (no payload) drives the UI refresh.

## Tests

Core (`reconcile.rs`):

- `compute_reconcile_desired` truth table: owned states kept, disabled/foreign
  dropped; newcomer auto-enables only with an enabled same-folder sibling; no
  cross-source / cross-kind sibling match.
- `reconcile_tool` on tempdirs: stale managed copy refreshed (stale → enabled),
  no-op when unchanged, newcomer auto-enabled beside an enabled sibling while a
  new-folder newcomer stays disabled, markdown rule body refreshed.
- `reconcile_all` skips disabled tools.
- `WorkspaceTargetStore::record_apply` upserts per tool.
