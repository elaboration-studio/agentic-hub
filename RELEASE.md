# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.2] — 2026-08-24

### Highlights

- **Spaced slash skills and SKILL.md name aliases are tracked** — `/grill me`,
  `/Repo Research`, and frontmatter names like `grilling` now resolve to the
  folder skill on Codex, Claude, and Cursor.
- **Kiro usage tracing** — when Kiro is enabled, Agentic Hub installs a managed
  tracer hook and attributes Kiro skill usage the same way as the other tools.

### Fixed

- **Spaced slash commands.** Prompt tokens were split on whitespace, so
  `/grill me` became `grill` and was dropped. Hyphen-joined slash tokens now
  resolve to the folder skill.
- **SKILL.md name aliases.** A frontmatter `name` that differs from the folder
  (`grilling` → `grill-me`) now maps to the installed skill.

### Added

- **Kiro tracer hook.** Managed install under `~/.kiro/hooks/`, camelCase
  `userPromptSubmit` payloads, and `.kiro/skills` repository catalog roots.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
