# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.3] — 2026-06-24

### Added

- **In-app auto-update.** Agentic Hub now updates itself with Tauri's built-in
  updater. It checks an R2-hosted feed on launch, on each app re-open, and
  weekly — throttled to once per 7 days — and offers a one-click **Install &
  Relaunch** when a newer signed release is available. Use **App ▸ Check for
  Updates…** to force an immediate check. Each release is signed with a minisign
  key and verified before it installs.

### Migration

- **None.** This release installs over 0.9.2 in place; the next update onward
  arrives automatically.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
