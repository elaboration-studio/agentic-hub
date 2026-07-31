# Agentic Hub 0.16.0 — Experience and Recovery Execution Plan

Status: Approved
Date: 2026-07-31
Feature: [v0-16-experience-and-recovery.md](../features/v0-16-experience-and-recovery.md)

## Global constraints

- Ship all six improvements together without feature flags or new dependencies.
- Preserve plan-then-apply, partial-tolerant projection behavior and existing suite/base/manual-extra semantics.
- Never add `tauri-plugin-shell`, force a stale repair, delete user files, or broaden automatic repair to broken/foreign states.
- The Rust core owns filesystem behavior; the React WebView stays untrusted; IPC remains typed and registered in capabilities.
- Shortcut strings are validated accelerators only. CLI probes are fixed bundled `program + args` data.
- Use TDD for stores, contracts, planners, and pure helpers; visually verify layout and framework wiring.
- Keep source files at or below 600 lines and update docs with shipped behavior.

## Task 1 — Shared Manager table and Suite scope

Extract the Manager matrix frame into reusable toolbar/filter/hierarchy/table pieces without changing Global or Workspace behavior. Add `suite` to frontend Manager scope, move Suites into the Manager rail, and render selected suite drafts through the shared table with an Included tri-state column. Preserve explicit Save, Cancel, Delete, Set base, tool selection, Apply Suite, `#/suites`, and Open Suites compatibility. Add Create from current using only enabled Hub-managed resources and expose missing references with removal.

Verify with focused store/component tests for filtering, tree behavior, suite selection, missing references, aliases, and Create from current exclusions.

## Task 2 — Guided stale-copy recovery

Preserve `suiteId` in frontend ownership state. Replace stale dots with an accessible recovery control. Unowned stale projections stage enabled for the normal ActionBar apply. Suite-owned stale projections call `cmd_resync_suite_binding(toolId)`, which reapplies the live selected suite, base suite, and manual extras without changing the binding. Keep Open source and Reveal target fallbacks.

Verify unowned staging, selected/base-suite re-sync, binding/manual-extra preservation, and missing-binding typed errors in Rust and UI tests.

## Task 3 — Today-first Statistics

Order tabs Today, Activity, Top usage, Unused, Inventory and default to Today. Keep Today local-day and range-independent. Place Usage overview above Most used in Top usage, move resource inventory into Inventory, hide the date selector on Today and Inventory, and keep Inventory usable while tracing is disabled.

Verify tab ordering/default, conditional date selector, and tracing-off Inventory behavior with focused frontend tests.

## Task 4 — Resources order and ripgrep catalog

Order Resources Skills, Tools, Sessions. Default to Skills only when skills.sh is enabled; otherwise default/fall back to Tools. Add ripgrep to the unchanged bundled catalog schema with ID `ripgrep`, program `rg`, args `--version`, no auth probe, and the official installation URL.

Verify rail order/default/fallback and bundled catalog parsing.

## Task 5 — Transactional direct palette shortcuts

Add generated `PaletteQuickSearchShortcuts` and `PaletteLaunchMode`. Extend settings with all-resources, skills, and commands shortcut defaults. Validate every accelerator and duplicate before persistence. Replace all OS registrations transactionally, restoring the prior set on failure and leaving settings unchanged. Keep hub toggle behavior; direct shortcuts always show/focus and set a one-shot mode consumed by `cmd_take_palette_launch_mode`. Keep Ctrl+1…7.

Verify legacy defaults, malformed/duplicate rejection, rollback, dispatch-to-mode, and visible-palette focus behavior in Rust and frontend tests.

## Task 6 — Release integration and verification

Generate shared types, reconcile docs/design/index/roadmap/backlog/registry, bump every 0.15.2 package/crate/Tauri/lockfile entry to 0.16.0, replace `RELEASE.md`, and move included Unreleased entries into `CHANGELOG.md` 0.16.0. Run focused tests per task, then `pnpm gen:types`, `pnpm test`, `pnpm build`, `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets --all-features --locked -- -D warnings`. Complete the approved manual Tauri smoke checklist, whole-branch review, push, and open a PR to `main`. Tag `v0.16.0` only after the PR is merged and the merge commit is on `main`.

## Manual smoke checklist

- Trigger all four global shortcuts from another app and while the palette is visible.
- Repair unowned, selected-suite, and base-suite stale managed copies.
- Create, edit, save, and apply a suite from the unified Manager.
- Verify Statistics and Resources with tracing and skills.sh both enabled and disabled.

## Security review card

- Threat snapshot: untrusted WebView and user-entered accelerators; trusted bundled CLI catalog and Rust filesystem core.
- Validation: accelerator parser + duplicate set check; typed tool/suite identifiers; central path validation for open/reveal and projection operations.
- Sensitive sinks: no shortcut executes a command; ripgrep uses the existing fixed direct probe; no new shell/plugin/dependency.
- Filesystem: stale repair cannot force takeover and remains within planner/applier semantics.
- Privacy: no new persisted data beyond shortcut settings and one-shot in-memory launch mode; existing local usage retention is unchanged.
