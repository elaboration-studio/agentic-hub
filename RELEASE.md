# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.11.1] — 2026-07-08

### Added

- **First-class internal hooks projection.** Agentic Hub-owned hooks (starting with
  the local usage tracer) appear as read-only `Agentic Hub` rows in the Manager
  so their projection state is visible alongside user source-root hooks.
- **Settings-managed hook sync preservation.** Enabled internal hooks are merged
  into every hook sync for their target tool, so Manager applies, suite full
  resets, and watcher reconcile cannot remove tracer hooks that are absent from
  user hook lists.

### Changed

- **Usage tracer hook definitions live in `internal_hooks`.** The Tauri usage
  collector delegates tracer manifests and paths to `agentic-core`, removing
  duplicated hook metadata from the hub crate.
- **Manager and suite UI lock internal hook toggles.** Settings-managed rows stay
  in sync payloads but cannot be toggled from the Manager matrix, suite editor,
  or command palette; Config remains the control surface for usage tracing.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
