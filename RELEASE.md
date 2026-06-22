# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.2] — 2026-06-22

### Added

- **Update skills.sh skills, right from the workspace inventory.** When you audit
  a project, every skill installed by the skills.sh CLI now carries a `skills.sh`
  badge (read from the project's `skills-lock.json`). Its row menu gains **Update
  via skills.sh**, which opens the install window in a focused update mode and
  runs `npx skills update` for that one skill — live output, Cancel, and an
  automatic re-scan when it's done. Scanning stays read-only; the update is an
  explicit, per-row action.

### Migration

- **None.** Projects without a `skills-lock.json` simply show no badges.

### Known Issues

- None.
