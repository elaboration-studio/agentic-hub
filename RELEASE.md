# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.8.1] — 2026-06-12

### Highlights

- **Fixed: suites no longer go empty after a git sync.** When the suites file
  lived at a custom path inside a git repo synced across machines, applying or
  editing a suite silently rewrote the file with device-specific source
  qualifiers. Two machines diverged, and a later `git pull` line-merged the
  multi-line capability arrays into nothing — the suite names survived but the
  resources vanished. Apply and palette "Apply suite…" are now pure reads: the
  synced file stops churning, so the merge that emptied it can't happen.
- **Belt-and-suspenders backups.** Every write to the suites and skill-favorites
  files first copies the prior good file to `<file>.bak`, so an accidental
  clobber is recoverable without reaching for `git checkout`.
- **Live reload after a pull.** The watcher now also watches the suites and
  favorites files. An external rewrite (a `git pull` on a synced path) reloads
  the Suites and Resources views immediately, so a stale in-memory snapshot can
  never overwrite freshly-pulled content.
- **Watcher on by default, toggle moved to Config.** The source watcher is an
  install-once preference, so its switch now lives in Config ▸ Source watcher
  rather than the header. A one-time migration flips any config that had paused
  the watcher back on; a deliberate pause after that still sticks.
- **Favorites parity with suites.** Your starred-skills file gets the same
  cross-device robustness — set a custom `favoritesPath` inside a git repo and
  share your favorite skills across machines, with backups and live reload.

### Migration

- **Automatic.** On first launch 0.8.1 force-enables the watcher once (recorded
  via a new `watcherForceMigrated` settings marker). No file formats changed and
  no manual steps are needed.

### Known Issues

- File-level watching of the suites/favorites files relies on path-based OS
  events (FSEvents on macOS). On Linux, a file replaced by rename may need the
  next app focus to refresh; the `.bak` backup and git history remain the
  recovery path either way.
