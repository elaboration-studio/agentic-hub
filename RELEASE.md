# Release notes

Notable changes per release. Newest first.

## Unreleased

### Added

- **Source Watcher.** The app now watches the configured source roots and, on any
  file change (e.g. after a `git pull`), automatically reconciles every enabled
  tool's projections from fresh source content and re-patches the active
  workspace, then live-refreshes the UI. New skills that land beside an enabled
  sibling auto-enable; new files in brand-new folders stay disabled. Foreign
  files/links are never taken over.
  - Header **Watch / Paused** toggle replaces the manual Rescan button
    (`watcherEnabled`, default on).
  - Config ▸ **Sync recovery** adds a "Rescan & resync everything" fallback.
  - New IPC: `cmd_set_watcher_enabled`, `cmd_rescan_resync`; new event
    `sources-changed`.
  - New core module `agentic-core::reconcile`; `Settings.watcherEnabled` and
    `WorkspaceTarget.lastApplied` added.
  - Docs: [features/source-watcher.md](docs/features/source-watcher.md),
    [tech/modules/watcher.md](docs/tech/modules/watcher.md).
