# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.6.2] — 2026-06-06

### Highlights

- **New OpenStandard tool.** Agentic Hub now manages the open-standard
  `~/.agents` directory as a first-class tool. Enable it (it's on by default)
  and your skills, agents, and rules project into `~/.agents/{skills,agents,rules}`
  — the shared convention other tools read — alongside Codex, Claude, Cursor,
  and OpenClaw. It shows up as its own column in the manager.
- **Codex is now self-contained under `~/.codex`.** Codex skills now live at
  `~/.codex/skills` (previously `~/.agents/skills`), so Codex's own resources and
  the shared open standard no longer overlap. The two are independent columns you
  can manage separately.

### Changes

- Added the `openstandard` tool adapter: skills and agents symlink into
  `~/.agents/`, rules write a managed block in `~/.agents/AGENTS.md`, and hooks
  (opt-in via a hook's explicit `targets`) write to `~/.agents/hooks.json`.
  Global-only, like OpenClaw.
- Changed the Codex default `skillsPath` from `~/.agents/skills` to
  `~/.codex/skills`.

### Migration

- **None required.** Existing configs load unchanged — the new `openstandard`
  block is injected automatically, and your saved Codex `skillsPath` is
  preserved. Only fresh installs (or configs that never set Codex's `skillsPath`)
  pick up the new `~/.codex/skills` default.

### Known Issues

- None.
