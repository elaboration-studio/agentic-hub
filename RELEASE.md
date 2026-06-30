# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.10.2] — 2026-06-30

### Fixed

- **Kiro rules now project to AGENTS.md, not per-file steering copies.** Shared
  rules sync into the managed block at `~/.kiro/steering/AGENTS.md` (always
  included per Kiro docs); native `.kiro/steering/*.md` files with inclusion
  modes are left to the user. Workspace inventory also attributes project-root
  `AGENTS.md` to Kiro, Copilot, and Antigravity alongside Codex/Cursor.
- **Copilot workspace inventory includes root `AGENTS.md`.** Copilot reads both
  `.github/copilot-instructions.md` and workspace-root `AGENTS.md`.
- **Antigravity workspace inventory includes `.agents/AGENTS.md`.** Antigravity
  loads both repo-root and `.agents/AGENTS.md` per Gemini/Antigravity docs.

### Added

- **Config → Tools lists all five projection targets per tool.** Skills, agents,
  rules, hooks, and commands show their on-disk write paths with a reveal-in-Finder
  action so you can verify what the hub projects.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
