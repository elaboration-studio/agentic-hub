# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.13.1] — 2026-07-17

### Added

- **Local usage tracing health checker.** The collector now verifies its own
  loopback health every hour, retries a stopped collector three times, and tells
  you to restart Agentic Hub only when recovery cannot restore local collection.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
