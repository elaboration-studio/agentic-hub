# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.4] — 2026-06-26

### Added

- **Inline per-tool toggle in the command palette.** Select a skill, agent, rule,
  or hook to drill into a sub-panel with live on/off state per tool — toggle
  immediately without a Manager round-trip. Includes **Enable/Disable for all
  tools**, **Open in editor**, and **Reveal in Finder**; suite-managed cells
  stay locked. **Alt+Enter / Alt+Click** still opens the source file.
- **Suite apply preserve mode.** When switching suites, preview manually enabled
  capabilities outside the effective suite and choose to remove or keep them.
- **Main window state persistence.** The hub remembers window size, position,
  and maximized state across launches.

### Changed

- **Suite apply manual extras.** Switching suites now tracks manually added
  capabilities separately from the previous suite's owned items. Preview shows
  only true manual extras; **Keep manually added** applies the new suite plus
  your extras and persists them on the binding.
- **Command palette polish.** Keyboard navigation scrolls the selected row into
  view; the palette dismisses cleanly after suite apply confirmation.

### Migration

- **None.** Installs over 0.9.3 in place; in-app auto-update delivers this
  release once published.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
