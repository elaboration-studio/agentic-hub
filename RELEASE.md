# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.12.0] — 2026-07-15

### Added

- **Statistics dashboard.** A new top-level tab beside Config shows your full
  agentic resource inventory — total resources, per-kind counts, enabled tools,
  and starred skills — alongside local usage charts, workspace breakdowns, a
  top-used table, and an unused-installed pruning list. Filter by 7d / 30d /
  90d / all-time; data stays on your Mac in `~/.agentic-hub/usage/trace.db`.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
