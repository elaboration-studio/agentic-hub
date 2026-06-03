# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.4.0] — 2026-06-03

### Highlights

- Apply a whole suite from the command palette: search a suite, drill in, and
  apply it to one tool as a full reset — and a persisted suite↔tool binding then
  auto re-syncs every bound tool whenever that suite's capabilities change.
- Suites are now portable across devices: each capability remembers the source
  it came from, so a synced suite resolves per-source and skips — never deletes
  or mis-resolves — capabilities whose source isn't present on the machine.
- Mark one suite as the base and its rules/skills merge into every applied
  suite, so your global resources are always present. The Manager locks the
  cells a suite manages and names the owning suite (and base) on hover.
- A smoother Manager: the filter bar and table header stay pinned while long
  lists scroll, locked cells read clearly, and enabled cells stand out.

### Changes

- Two-level palette: a suite row drills into a suite-tools view (`‹ <suite>`
  breadcrumb, Backspace-to-back) where each tool row runs a full-reset apply.
- New `~/.agentic-hub/suite-bindings.json` records suite↔tool bindings;
  `cmd_apply_suite` records, `cmd_update_suite` re-applies bound tools (guarded
  by the reconcile lock, emits `sources-changed`), `cmd_delete_suite` drops them.
- Every scanned `CapabilityItem` carries a portable `source` (`SourceRef`:
  home-relative path + folder); suite entries are now source-qualified
  (`SuiteCapabilityRef { cap, source }`). Apply resolves refs source-aware and
  reports `ApplySuiteResult.skippedAbsentSource` for refs whose source is absent.
- Suites carry a portable `isBase` flag (at most one base, enforced by the
  store). Every global apply unions the base via `merge_base_caps`; the recorded
  binding stays the selected suite. New `cmd_set_base_suite` re-syncs all bound
  tools, and editing the base re-syncs every binding. `cmd_suite_ownership`
  surfaces which suite owns each `(tool, item)` so the Manager can lock the cell.
- Manager: the toolbar and table header are sticky over a scrolling list;
  suite-locked cells show an indigo dashed lock with a not-allowed cursor; the
  enabled-cell highlight is clearer. Applying a suite or changing the base now
  refreshes the matrix so its state and locks stay in sync.

### Migration

- `~/.agentic-hub/suite-bindings.json` is created on first suite apply; a missing
  file is treated as empty, so existing installs need no migration.
- Suite files (`~/.agentic-suites.json`) with legacy bare-string capabilities
  load unchanged and upgrade to the qualified object form on the next save —
  non-breaking, no manual migration.
- The new suite `isBase` flag defaults to `false`, so existing suite files load
  unchanged with no base set — non-breaking.
