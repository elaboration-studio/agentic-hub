# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.4] — 2026-08-25

### Highlights

- **Cmd+Alt+A no longer brings the hub window with the palette.** If the hub
  was hidden, search stays on top by itself.

### Fixed

- **Palette summon no longer unhides the hub.** Closing the hub hides the
  whole app; showing the palette then unhid NSApp and Dock-reopen focused the
  main window on top of search. The palette now skips that reopen and hides
  the hub again if it was not already visible.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
