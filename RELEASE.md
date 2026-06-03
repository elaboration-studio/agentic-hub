# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.3.0] — 2026-06-03

### Highlights

- Command palette: an Alfred-style floating panel summoned by a configurable
  global shortcut (default Cmd+Alt+A) that searches resources and opens the
  original file in your editor — plus native macOS menus with Cmd+, for Config.
- Palette suite apply: search a suite, drill in, and apply it to one tool as a
  full reset — and a persisted suite↔tool binding auto re-syncs every bound tool
  whenever a suite's capabilities change.
- Source-aware suites: suites now remember which source each capability came
  from, so a suite synced across devices resolves per-source and skips — never
  deletes or mis-resolves — capabilities whose source isn't on the machine.
- Base suite: mark one suite as the base and its rules/skills merge into every
  applied suite, so your global resources are always present. The Manager locks
  cells a suite manages and names the owning suite (and base) on hover.

### Changes

- New floating `palette` window toggled by a global accelerator
  (`Settings.paletteShortcut`, default `Cmd+Alt+A`), dismissed on blur or Esc.
- Flat resource search opens the original file via the existing opener allowlist;
  an extensible command-provider registry also ships navigation commands.
- Native application menu (App / Edit / View / Window); "Settings…" (Cmd+,) routes
  to Config and "Command Palette" toggles the panel. Cmd+Q stays the hard exit.
- Config gains a Command Palette panel to edit the shortcut; saving re-registers it.
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

### Migration

- The new `paletteShortcut` field defaults to `Cmd+Alt+A` for existing configs.
- `~/.agentic-hub/suite-bindings.json` is created on first suite apply; a missing
  file is treated as empty, so existing installs need no migration.
- Suite files (`~/.agentic-suites.json`) with legacy bare-string capabilities
  load unchanged and upgrade to the qualified object form on the next save —
  non-breaking, no manual migration.
- The new suite `isBase` flag defaults to `false`, so existing suite files load
  unchanged with no base set — non-breaking.
