# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.0] — 2026-07-19

### Added

- **Repository-local usage tracing.** Skills used from active Cursor, Claude
  Code, and Codex repositories now receive their own local usage counts even
  when the repository was never saved in Agentic Hub.
- **Complete multi-skill capture.** Every distinct skill used in one turn is
  recorded once, while repeated references and overlapping hook events remain
  deduplicated.
- **Tracing diagnostics.** Config shows hook installation, the latest captured
  event, and resolved/unresolved totals for each supported tool.

### Changed

- Statistics preserves repository context for historical local usage, including
  repositories that are not currently selected in Manager.
- Local usage data now distinguishes same-named global and repository skills.

### Fixed

- Generic slash commands, clipboard files, images, and arbitrary paths no
  longer appear as unresolved skill usage.
- Existing unresolved local history is reconciled when its repository still
  exists and the skill can be identified without ambiguity.

### Privacy and security

- Prompts, tool arguments, skill contents, tokens, and secrets are never stored
  by usage tracing. Repository paths are canonicalized and bounded to documented
  skill roots before any skill attribution occurs.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
