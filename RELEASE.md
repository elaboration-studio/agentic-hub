# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.5] — 2026-06-26

### Added

- **Paste into focused app (macOS).** Turn on **Paste into focused app** in
  Config. When enabled, selecting a command in the palette copies its body and
  pastes it straight into the app you were using — Alfred-style, no manual
  Cmd+V. Requires Accessibility permission for Agentic Hub; off by default.

### Migration

- **None.** Installs over 0.9.4 in place; in-app auto-update delivers this
  release once published.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
