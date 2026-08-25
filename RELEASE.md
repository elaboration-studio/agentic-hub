# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.3] — 2026-08-25

### Highlights

- **Grok Build is a first-class tool.** Opt in the Grok column to project
  skills, agents, rules, commands, and hooks into `~/.grok/`. Usage tracing
  attributes Grok turns the same way as Cursor, Claude, Codex, and Kiro.

### Added

- **Grok adapter.** Skills (nested symlinks), agents and commands (flat
  `*.md`), rules (nested, `.mdc` → `.md`), and per-id Claude-style hook JSON
  under `~/.grok/hooks/`. Grok is disabled by default until you enable it in
  Config. Default hook targets now include Grok; hooks whose `targets` omit
  `"grok"` stay out of the Grok column.
- **Grok usage tracing.** When tracing is on and Grok is enabled, a managed
  tracer hook records `promptId` turns, `read_file` SKILL.md reads, and
  qualified slash names (`/user:commit`).

### Fixed

- **Broken Grok hook files are never overwritten.** Invalid JSON under
  `~/.grok/hooks/` is left as-is and reported instead of replaced.
- **Open / Reveal works on Grok paths.** Skill folders and hook files under
  `~/.grok/` are on the open allowlist.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
