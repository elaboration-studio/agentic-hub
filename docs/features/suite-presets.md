# Feature: Suite Presets

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md)
Related Docs: [docs/tech/modules/suite-presets.md](../tech/modules/suite-presets.md), [docs/features/workspace-inventory.md](./workspace-inventory.md)

## Why now

The MVP capability manager enables per-item toggling. This works for small deltas but forces users to manually reconstruct entire configurations when switching between scenarios — coding, research, writing, debugging. There is no save/restore mechanism today.

Suites collapse "switch context" into one action: pick a named preset, apply it. The focused tool resets to exactly the capabilities the suite defines. After that, the user can free-edit as before.

## User story

As a power user managing multiple AI tools, I want to save a named capability configuration as a suite and apply it to any tool in one click, so that I can switch between work scenarios instantly without re-toggling individual items.

## Scope

### In scope

- Suite data model: named, tool-agnostic list of capability IDs
- Suite storage: `~/.agentic-suites.json` (single portable dotfile, parity path with VS Code extension)
- Suite Manager window: standalone Tauri window for creating, editing, renaming, and deleting suites
- "Create from current" shortcut: capture the currently enabled capabilities for the focused tool as a new suite
- Main window integration: suite selector in header, "Apply Suite" action
- Apply semantics: full reset — disable everything for the focused tool, then enable only the suite's capabilities, then run projection sync
- Post-apply free editing: after a suite apply, the user can toggle individual items normally
- Validation: surface warnings when a suite references capabilities that no longer exist in the shared root

### Out of scope

- Per-tool overrides inside a suite definition
- Suite versioning or history
- Suite import/export to external formats
- Suite composition (combining multiple suites)
- Suite sharing across machines
- Automatic suite application on app startup
- Undo/rollback of a suite apply (user can re-apply a different suite or manually edit)

## Experience

### Suite Manager window

Opened via a top-bar button in the main window ("Manage Suites…") or via the suite selector dropdown's "Manage Suites…" option. Lives in a separate Tauri window so the user can have both open side-by-side.

```
+----------------------------------+-------------------------------------+
| Suite List                       | Suite Editor                        |
|                                  |                                     |
| + New Suite                      | Name: [________________]            |
|                                  | Description: [________________]     |
| > coding-workflow      12 items  |                                     |
|   research-mode         8 items  | Capabilities                        |
|   writing-focus         5 items  | [ ] skill:dev/repo-research         |
|                                  | [x] skill:tdd                       |
|                                  | [x] agent:coding/coding-agent       |
|                                  | [!] rule:legacy/old-rule (stale)    |
|                                  | [ ] rule:general/precise            |
|                                  |                                     |
|                                  | [Save] [Delete] [Rename] [Duplicate]|
+----------------------------------+-------------------------------------+
```

Behaviors:
- **New Suite**: empty suite, or "Create from current" pre-fills with the currently enabled set for a selected tool
- **Edit**: searchable checklist of all capabilities from the latest shared-root scan
- **Delete**: confirmation before removal
- **Rename**: inline rename on the name field (uniqueness check)
- **Duplicate**: copy an existing suite as a new draft
- **Validation**: items that exist in the suite but not in the latest scan show a warning badge

### Main window integration

The capability manager header gains a suite selector:

```
Header
  Agentic Hub      Shared root: ~/.agentic   [ Global | Workspace ]
  Tool tabs:  [Codex] [Claude] [Cursor] [OpenClaw]
  Suite: [ coding-workflow ▾ ]   [Apply Suite]
  Manage Suites…
  Refresh   Settings
```

Behaviors:
- Dropdown lists all saved suites plus "Manage Suites…" (opens Suite Manager window)
- "Apply Suite" is enabled only when a suite is selected and a tool tab is focused
- Clicking "Apply Suite" shows a confirmation: "Reset all {Tool} capabilities to match suite '{Suite}'?"
- On confirm: full reset and apply through the existing plan/apply pipeline
- After apply: staged state clears, inventory refreshes, result summary toast appears

### Apply flow (user perspective)

1. User selects a tool tab (e.g., Cursor).
2. User picks a suite from the dropdown.
3. User clicks "Apply Suite".
4. Confirmation dialog shows suite name and tool.
5. On confirm:
   - All existing projections for the tool are removed (or replaced).
   - Only capabilities listed in the suite are enabled.
   - Projection sync runs (symlinks, managed copies, markdown sections).
6. Inventory refreshes. Result summary shows applied / skipped-stale / errors.
7. User can now toggle individual items as usual.

### Handling stale references

When a suite references a capability ID that no longer exists in the shared root:
- The Suite Manager editor shows a warning icon next to the stale entry.
- During apply, stale references are silently skipped (they cannot be projected).
- The apply result summary notes how many suite items were skipped due to missing capabilities.

## Acceptance criteria

- [ ] A "Manage Suites…" affordance opens the Suite Manager window
- [ ] Suite Manager lists all suites from `~/.agentic-suites.json`
- [ ] Creating a new suite writes it to the dotfile and shows it in the list
- [ ] "Create from current" captures all enabled capabilities for the focused tool
- [ ] Editing a suite updates the capability checklist and saves on explicit Save
- [ ] Deleting a suite removes it from the dotfile after confirmation
- [ ] The main window header shows a suite selector dropdown listing all suites
- [ ] Selecting a suite and clicking "Apply Suite" triggers a confirmation dialog
- [ ] On confirm, all existing projections for the focused tool are removed or replaced
- [ ] Only capabilities in the suite are enabled after apply
- [ ] Projection sync runs correctly for all kinds (symlinks, managed copies, markdown sections)
- [ ] The apply result summary shows applied, skipped, and error counts
- [ ] Stale suite references are skipped with a summary note
- [ ] After suite apply, user can toggle individual items normally
- [ ] Suite data persists across app restarts
- [ ] Suite name uniqueness enforced at create / rename time
- [ ] Atomic dotfile writes via `.tmp` + rename
- [ ] Cross-window event broadcast refreshes the main window's suite dropdown when Suite Manager creates / edits / deletes a suite

## Dependencies

- Existing `agentic-core::scanner` for the capability inventory
- Existing `planner` and `applier` for projection changes
- Existing `rule_sync` for markdown section projection
- New `agentic-core::suite_store` for `~/.agentic-suites.json` CRUD
- New IPC commands: `cmd_list_suites`, `cmd_get_suite`, `cmd_create_suite`, `cmd_update_suite`, `cmd_delete_suite`, `cmd_apply_suite` (see [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md))
- Tauri event: `suite-store-changed` (emitted globally; both windows subscribe)

## Delivery slices

| Slice | What ships | Why this cut |
|-------|-----------|--------------|
| V1: Storage + Service | `suite_store` reads/writes `~/.agentic-suites.json`. Types added. No UI yet. | Foundation that both windows depend on. |
| V2: Suite Manager window | Standalone Tauri window with full CRUD. "Create from current" works. | Users can define suites before applying them. |
| V3: Main window integration | Suite selector dropdown, "Apply Suite" button, confirmation dialog, full-reset apply flow. | The core value: one-click scenario switching. |
| V4: Validation + polish | Stale reference warnings, apply result details for skipped items, error hardening. | Trust and reliability before daily use. |

## Risks and edge cases

- **Large capability sets** — suites with 100+ items should not cause apply timeouts. The existing apply pipeline handles items sequentially; performance is validated in M2 integration tests.
- **Concurrent window edits** — if both Suite Manager and main window are open, suite apply on the main window must not conflict with an in-progress edit. The dotfile is re-read on each apply. The cross-window event broadcast updates the main window's dropdown after a save.
- **Missing dotfile** — first save creates `~/.agentic-suites.json` with `{ "version": 1, "suites": [] }`.
- **Disk permission errors** — dotfile write failures surface clearly; in-memory state is preserved.
- **Suite name collisions** — UI prevents duplicates at create / rename time.
- **Empty suite apply** — applying an empty suite disables all capabilities for the focused tool. Valid but the confirmation dialog uses a specific message ("This will disable all capabilities for {Tool}").

## Metrics or signals

- Frequency of suite apply vs. per-item apply (suite should become dominant for multi-item changes)
- Number of suites created per user (signals adoption vs. abandonment)
- Suite apply error rate (target near zero after V4)

## Open questions

- Should suite apply show a dry-run preview before confirmation, or is suite name + tool sufficient? Current design: name + tool is sufficient; the apply result is the after-the-fact preview.
- Should the Suite Manager allow drag-and-drop reordering? Defer; alphabetical sort is sufficient in v1.
- Suite IDs are UUIDs (stable under rename) vs slugs (human-readable in dotfile). Current decision: UUIDs.
