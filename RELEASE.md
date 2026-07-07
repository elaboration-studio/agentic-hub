# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.11.0] — 2026-07-07

### Added

- **Local usage tracing.** Opt-in, local-only tracing records explicit skill,
  agent, and command usage into `~/.agentic-hub/usage/trace.db`. The Manager
  matrix adds a **Usage** column with per-tool hover breakdowns so you can see
  which capabilities Codex, Claude Code, Cursor, and other enabled tools actually
  invoke.
- **Agent usage counts.** Agent specs are attributed when invoked via explicit
  slash commands (`/cto`), Claude `@agent-*` mentions, or a single Read of an
  agent markdown file under an `/agents/` path.
- **Command palette usage counts.** Copy and paste actions from the global
  command palette are recorded against command capabilities.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
