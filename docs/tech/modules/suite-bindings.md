# Module: Suite ↔ Tool Bindings

Status: Implemented
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-03
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/suite-presets.md](./suite-presets.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md), [docs/features/command-palette.md](../../features/command-palette.md)

## Purpose

Track which suite is currently applied to each tool in global scope, so that
editing a suite's capabilities automatically re-syncs every tool that suite was
applied to. Without this, applying a suite is a one-shot snapshot — a later
capability add/remove would silently drift from the tools.

## Model

```rust
pub struct SuiteBinding {
    pub tool_id: ToolId,
    pub suite_id: String,
    pub manual_item_ids: Vec<String>, // ids enabled beyond the effective suite
}
```

One binding per tool. A suite apply is a **full reset** (`api::apply_suite`):
after it runs, a tool reflects exactly one suite's set. Re-applying a different
suite to the same tool replaces the prior binding (upsert by `tool_id`).

## Storage

`SuiteBindingStore` persists to `~/.agentic-hub/suite-bindings.json` — a new
dotfile alongside the workspace-target state, kept separate so the two stores
never contend:

```json
{ "version": 1, "bindings": [ { "toolId": "codex", "suiteId": "backend", "manualItemIds": ["skill:extra"] } ] }
```

Contract (mirrors `WorkspaceTargetStore`):

- **Atomic writes** — write to `*.json.tmp`, then rename.
- **Missing file** → empty bindings (never an error).
- **Malformed file** → `StateParse` error; the file is **never overwritten**,
  so a hand-corrupted file is preserved for inspection.

API: `read()`, `get(tool)`, `record(tool, suite_id, manual_item_ids)` (upsert by tool),
`tools_for_suite(suite_id) -> Vec<ToolId>` (in `ToolId::ALL` order),
`drop_suite(suite_id)`.

## Lifecycle

```mermaid
flowchart TD
  apply["cmd_apply_suite(tool, suite)"] --> merge["effective = suite ∪ base"]
  merge --> reset["api::apply_suite (full reset)"]
  apply --> rec["store.record(tool, suite)"]
  upd["cmd_update_suite (caps changed)"] --> which{"is base?"}
  which -- "no" --> tf["tools bound to this suite"]
  which -- "yes" --> allb["every binding"]
  tf --> resync["resync_bindings (base-merged, guarded) → emit sources-changed"]
  allb --> resync
  setb["cmd_set_base_suite(id?)"] --> allb
  del["cmd_delete_suite"] --> drop["store.drop_suite(id) (projections untouched)"]
```

- **Apply** (palette or Suites page): merge the base suite into the selected
  suite, full-reset apply, then record the binding to the **selected** suite
  (the base is never the recorded binding).
- **Update** (capability edit): re-apply as a full reset, base-merged, serialized
  against the filesystem watcher via the reconcile guard
  (`watcher::with_reconcile_guard`) so the two never write the same tool dirs.
  Editing a **normal** suite re-syncs only the tools bound to it; editing the
  **base** suite re-syncs **every** binding (each with its own selected suite
  re-merged). Emits `sources-changed` so the Manager refreshes.
- **Set base** (`cmd_set_base_suite`): flip the single-base flag, then re-sync
  every binding so all tools pick up (or drop) the new base.
- **Delete**: drop the suite's bindings only. Deleting a suite is not a
  destructive tool wipe — on-disk projections are left as they are.

`resync_bindings` resolves each binding's selected suite fresh, unions the
current base via `api::merge_base_caps`, and full-reset applies per tool under
one reconcile guard.

## Cross-device note

Bindings map `toolId -> suiteId` (a portable UUID), so the bindings file syncs
across devices unchanged. The suite itself carries source-qualified refs (see
[suite-presets.md](./suite-presets.md)); on re-apply, refs whose source is
absent on the current machine are skipped and preserved — a synced binding
never deletes or mis-resolves a capability that belongs to a source the device
does not have.

## Scope boundary

This binding covers **global-scope** tools only. Workspace scope is a read-only
inventory (see [workspace-inventory.md](./workspace-inventory.md)) and has no
bindings, so the two mechanisms do not overlap.

## Tests

- Core store (`suite_binding_store.rs`): record upserts per tool, re-record
  replaces, `tools_for_suite` filters and orders, `drop_suite` removes only its
  bindings, missing-file empty, malformed-not-overwritten.
- Apply accuracy (`api.rs`): re-apply with a smaller suite removes the dropped
  managed copy; rules block rewrites to exactly the suite set; hooks reflect the
  suite set; empty suite disables everything; stale managed copy refreshes;
  `apply_suite_to_tools` applies to each listed tool.
