# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.10.0] — 2026-06-30

### Added

- **Kiro tool adapter.** Project shared skills, agents, steering rules, and
  hooks into `~/.kiro/` when enabled in Config (off by default). Hooks require
  `"targets": ["kiro"]`. Workspace inventory scans project `.kiro/` dirs.
- **GitHub Copilot tool adapter.** Project skills, custom agents (`.agent.md`),
  instruction rules (`.instructions.md`), and hooks into `~/.copilot/` when
  enabled (off by default). Hooks require `"targets": ["copilot"]`. Workspace
  inventory scans `.github/` Copilot dirs.
- **Google Antigravity tool adapter.** Project skills, rules (managed block in
  `~/.gemini/AGENTS.md`), and hooks (`~/.gemini/config/hooks.json`) when enabled
  (off by default). Agents and commands are unsupported. Hooks require
  `"targets": ["antigravity"]`. Workspace inventory scans `.agents/` dirs.

### Migration

- **None.** Installs over 0.9.5 in place; in-app auto-update delivers this
  release once published. New adapters are off by default — enable each tool in
  Config when you are ready.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
