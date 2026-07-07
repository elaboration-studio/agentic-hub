# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.10.3] — 2026-07-04

### Added

- **Local skill usage tracing.** Opt-in, local-only tracing records explicit
  skill/tool completion events into `~/.agentic-hub/usage/trace.db` and adds a
  Manager Usage column with per-tool hover breakdowns.
- **Global Manager shows unmanaged tool installs.** The Global view merges Hub
  source-root resources with read-only rows discovered from enabled tools'
  native global folders (`~/.codex`, `~/.claude`, `~/.cursor`, and peers).
  Unmanaged rows are labeled by tool source, filterable via the Source filter,
  and support Open / Reveal without toggles, Apply, or sync. Duplicates already
  represented by Hub-managed projection state are suppressed.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
