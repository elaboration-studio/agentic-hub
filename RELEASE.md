# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.5] — 2026-09-08

### Highlights

- **Search opens where you are working.** `Cmd+Alt+A` now centers the command
  palette on the display containing the focused app/window.
- **The hub stays in the window state you left it.** Dismissing search restores
  an app-hidden or focused hub correctly and does not reorder it when summoned
  over another app.

### Fixed

- Active-display placement now moves the native panel onto the focused screen
  before AppKit centers it.
- Palette presentation tracks app-hidden, focused-main, and external-app
  origins, restoring each state on dismiss without leaving a visible,
  windowless application.
- Revision tokens prevent an older blur/dismiss callback from cancelling a
  newer palette summon.

### Migration

- None.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
