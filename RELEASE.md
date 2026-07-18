# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.14.0] — 2026-07-18

### Added

- **Statistics page sub-tabs.** Usage data is now split into Overview,
  Activity, Top usage, and Unused tabs instead of one long scroll — inactive
  tabs don't render, so charts and tables only mount when you open them.
- **Today's usage table.** The Overview tab now shows which capabilities were
  used today (local time), independent of the selected date range.

### Changed

- **Local usage tracing is now on by default.** New installs, and any config
  file predating this setting, start with local skill/tool usage tracing
  enabled — this writes managed tracer hooks into your Codex/Claude/Cursor
  configs and stores events in a local SQLite database. Nothing leaves your
  machine, and it can be turned off in Config → Local usage tracing.
- Timestamps across Statistics and the Manager matrix now render in your
  local time instead of raw UTC.

### Fixed

- The "Today's usage" boundary now uses the machine's local calendar day
  instead of UTC, matching the locally-formatted timestamps shown next to it.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
