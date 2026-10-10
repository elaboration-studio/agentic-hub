# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.17.0] — 2026-10-10

### Highlights

- **Agent bundles** let a suite ship instructions, required CLIs, and MCP servers as an immutable per-harness bundle under `~/.agentic-hub/bundles`.
- **`ehub` CLI** — the app links `~/.agentic-hub/bin/ehub` to itself; use `ehub bundle` to build and refresh bundles for eCanvas eHub runs.

### Added

- Suite agent block (emoji, instructions, required CLIs) and MCP server definitions flow into versioned bundle directories.
- Streaming hash + staging GC for bundle writes.

### Pair with eCanvas

- eCanvas **0.8.5** eHub agent profiles expect this build (or newer) on the Mac that mounts bundles.
