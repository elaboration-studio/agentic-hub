---
name: resolve-foreign-file-conflicts
overview: "Add a confirmed, destructive \"take over the target\" path so a foreign_file conflict (a real file blocking a tool projection) can be resolved from the UI: at Apply time, if the user has enabled items whose target is blocked, warn and on confirm delete the blocking file/dir and project."
todos:
  - id: core-plan-force
    content: "Planner: add force param to build_plan/diff_op; ForeignFile+desired+force emits ReplaceLink/ReplaceManagedCopy take-over"
    status: completed
  - id: core-apply-force
    content: "model: add PlannedOperation.force; applier: ReplaceLink honors force (delete real target then symlink); expose managed_copy::remove_existing"
    status: completed
  - id: core-callers
    content: Thread force through api::plan and cmd_plan PlanInput; pass force=false in apply_suite and reconcile
    status: completed
  - id: core-tests
    content: Add planner + applier take-over unit tests (with and without force)
    status: completed
  - id: types-ipc
    content: Regenerate ts-rs types (PlannedOperation.force); add force arg to ipc.ts plan()
    status: completed
  - id: ui-conflict-dialog
    content: "App.tsx: detect foreign_file conflicts at Apply, ConflictDialog warning modal + styles, thread force into plan calls"
    status: completed
  - id: docs-release
    content: Update IPC contract + projection doc; add RELEASE entry
    status: completed
  - id: verify-mr
    content: Run tests/clippy/build, manual smoke, commit and open MR to main
    status: completed
isProject: false
---

## Resolve foreign_file conflicts (delete-and-take-over, confirmed)

### Root cause
`foreign_file` = a real, non-managed file/dir sits at a tool's projection target. The planner maps `(ForeignFile, true)` to `SkipConflict`, and the applier treats `SkipConflict` as a no-op, so checking + Apply changes nothing.

```260:261:crates/agentic-core/src/planner.rs
        (ForeignFile, true) => (SkipConflict, "A real file blocks projection", false),
```

### Decision (locked with user)
- Destructive: delete the blocking file/dir, then project (strategy B).
- Gated by an explicit warning dialog at Apply time (trigger: apply-confirm).
- Authorization rides the existing plan -> apply pipeline via a `force` flag (no parallel pipeline; honors the repo's "reuse plan/apply" principle and "never silently overwrite" — the take-over is explicit and confirmed).

### Core (Rust)

1. `PlannedOperation.force` — add `#[serde(default)] pub force: bool` in [crates/agentic-core/src/model.rs](crates/agentic-core/src/model.rs). Update the four struct literals (planner `diff_op`, applier test helper, any others) to set it.

2. Planner — [crates/agentic-core/src/planner.rs](crates/agentic-core/src/planner.rs): add `force: bool` to `build_plan` and `diff_op`. When `force`, change only the `(ForeignFile, true)` arm to a take-over instead of `SkipConflict`:
   - managed (file_sync) -> `ReplaceManagedCopy`, `with_source = true`, `force = true`.
   - link (link_sync) -> `ReplaceLink`, `with_source = true`, `force = true`.
   All other arms unchanged; `force = false` reproduces today's behavior exactly. `resolve_target_collisions` still runs after.

3. Applier — [crates/agentic-core/src/applier.rs](crates/agentic-core/src/applier.rs): in the `ReplaceLink` arm, when `op.force`, skip the `is_symlink_or_managed_copy` guard and remove whatever is at the target (file/dir/symlink) before symlinking. `ReplaceManagedCopy` already overwrites real targets via `managed_copy::write_managed_copy` (its `remove_existing` handles real dirs), so no change there. Reuse the delete by making `managed_copy::remove_existing` `pub(crate)`.

4. Thread `force` through callers, defaulting to `false` where take-over must never happen:
   - `api::plan` ([crates/agentic-core/src/api.rs](crates/agentic-core/src/api.rs)) gains a `force` param.
   - `api::apply_suite` (line ~213) and `reconcile.rs` (lines ~118, ~262, the watcher) pass `force = false`.

5. `cmd_plan` ([crates/agentic-hub/src/commands.rs](crates/agentic-hub/src/commands.rs)): add `#[serde(default)] pub force: bool` to `PlanInput`, pass to `api::plan`.

### IPC + types
6. Regenerate ts-rs (`PlannedOperation` now has `force`): `cargo test -p agentic-core --features ts-export`.
7. [src/ipc.ts](src/ipc.ts): `plan(toolId, items, desired, force = false)` adds `force` to the `cmd_plan` payload.

### UI (React)
8. [src/App.tsx](src/App.tsx) `applyChanges`: before planning, compute conflicts from current state — items where `currentMap.get(key(tool,item.id))?.state === "foreign_file"` and `desired[key] === true`, across the modified tools. If any, open a `ConflictDialog` listing each (tool label, capability name, `state.targetPath` in monospace) with a warning.
   - "Delete & take over" -> run the apply loop with `force = true`.
   - "Skip these" / dismiss -> run with `force = false` (other changes still apply; conflicts stay blocked).
   - Implement as a promise the dialog settles; no changes to the per-tool loop other than passing `force` into `plan(...)`.
9. Add `ConflictDialog` component + a small modal/overlay style in [src/styles.css](src/styles.css) (danger-toned, lists targets).

### Docs
10. [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md): document `cmd_plan` optional `force` and the take-over semantics; add `PlannedOperation.force`.
11. [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md): note the explicit, confirmed `foreign_file` take-over (real files are still never overwritten *silently*). Append a `RELEASE.md` Unreleased entry.

### Tests
12. Planner: `(ForeignFile, true)` with `force` emits `ReplaceLink`/`ReplaceManagedCopy` (force=true, source set); without `force` still emits `SkipConflict`.
13. Applier: `ReplaceLink` with `force` over a real file and a real dir removes then symlinks; without `force`, a real file at a `ReplaceLink` target still errors `conflict_real_file_at_target`. `ReplaceManagedCopy` with `force` overwrites a real dir.

### Verify + MR
14. `cargo test --workspace`, `cargo clippy --all`, `pnpm build` (tsc + vite). Manual smoke: create a real file at `~/.claude/skills/<x>`, see `foreign_file`, check it, Apply -> dialog -> confirm -> projected. Branch off main, commit, open MR (no direct commits to main).

### Out of scope
- Take-over for Suite Apply and Workspace Patch (full-reset paths keep skipping `foreign_file`; can follow up).
- Backup-before-delete (user chose destructive); no `.bak` is written.
- A per-row take-over action (resolution is the Apply-time dialog only).