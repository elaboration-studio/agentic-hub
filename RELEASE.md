# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.10.1] — 2026-06-30

### Fixed

- **Kiro projection used symlinks Kiro cannot load.** Skills, agents, and
  steering rules now hard-copy with flat layout where required (same strategy
  as Claude Code skills).
- **Copilot and Antigravity skills used nested paths their loaders never scan.**
  Copilot skills now flatten to the top level (symlinks still work); Antigravity
  skills hard-copy with flat layout because Antigravity ignores symlinks.
- **Antigravity default skills path was wrong.** Default is now
  `~/.gemini/config/skills` (was `~/.gemini/skills`).

### Migration

- **Antigravity skills path (one-time).** On first launch after updating, configs
  still pointing at the legacy default `~/.gemini/skills` are rewritten to
  `~/.gemini/config/skills`. Custom paths are untouched. Re-apply or reconcile
  Antigravity after updating if you had already enabled it under the old path.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
