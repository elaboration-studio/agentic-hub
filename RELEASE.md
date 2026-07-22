# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.15.2] — 2026-07-22

### Highlights

- **Install skills.sh skills into your library** — one install normalizes into
  your Hub source root and projects everywhere, not just a single workspace.
- **App-wide appearance** — choose Light, Dark, or Follow system; every window
  and native chrome updates immediately.
- **More reliable usage tracing** — Cursor slash skills, resilient event
  delivery, and clearer Codex/repo-local resolution.

### Added

- **Library-scope skills.sh install.** The install window adds a Workspace |
  Library toggle. Library installs stage into a configured Hub source root,
  normalize to the shared `skills/` layout, and record in that root's
  `skills-lock.json`. Library-installed skills are badged in the Global Manager
  with **Update via skills.sh**, matching workspace scope.
- **App-wide color scheme.** Config offers Light, Dark, and Follow system.
  The choice applies immediately across the main app, command palette, install
  window, and native chrome.

### Fixed

- **Statistics chart tooltips** are readable on the dark chart theme (label,
  item, and hover cursor colors).
- **Cursor `/skill` usage tracing** from prompt-submit hooks — slash skills,
  agent mentions, and agent markdown reads under `/agents/` — without relying
  on a secondary Skill tool or `SKILL.md` attachment.
- **Delivery resilience** — failed tracer events spool under
  `~/.agentic-hub/usage/spool/` and drain on collector start and every 30s.
- **Codex name precedence** — same-named global and repository skills resolve
  workspace-first, aligned with Cursor.
- Repository-local agents under documented tool agent dirs are included in the
  usage catalog for `/agent` and agent `Read` resolution.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
