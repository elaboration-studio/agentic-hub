# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.13.0] — 2026-07-17

### Added

- **App-wide color scheme.** Choose Light, Dark, or Follow system in Config.
  The app updates immediately and keeps its main window, command palette, and
  install window synchronized. New installations follow the operating system;
  existing installations retain their current dark appearance until you choose a
  different option.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
