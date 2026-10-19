# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.18.0] — 2026-10-19

### Highlights

- **Discover skills for a specific harness.** Use `ehub skills list --harness <cursor|claude|codex|grok>` to get a JSON inventory of the skills available to that CLI.
- Add `--workspace <absolute-directory>` to include project skills alongside eHub and installed tool skills.

### Added

- Skill descriptions, canonical paths, and source scopes in the CLI inventory, with duplicate paths removed.
- Validation of harness names and workspace directories before listing skills.

### Pair with eCanvas

- Keeps the agent-bundle and `ehub` CLI contract introduced in 0.17.0, and adds skill discovery for local agent runs.
