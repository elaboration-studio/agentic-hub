# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.19.0] — 2026-10-20

### Highlights

- **Local runs can be attributed in the usage tracker.** An eCanvas-assigned Codex, Claude, Cursor, or Grok run records its task, run, and suite version, and skill events from that run attach to it.
- A missing tracer hook or disabled tracing is an explicit untracked result.

### Added

- `POST /runs` on the loopback collector, and run identity on usage events.
- The trace database adds those columns on open. Existing events stay in place.

### Pair with eCanvas

- eCanvas announces the run and stamps the CLI. That app update ships separately. This build is the collector that accepts it.
