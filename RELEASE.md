# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.1] — 2026-08-03

### Highlights

- **Skill usage attribution is consistent across Codex, Claude, and Cursor**
  — installed projections no longer collide with their source names, and
  ambiguous invocations are kept for reconciliation instead of being dropped.

### Fixed

- **Cross-tool skill attribution.** When a tool held the same name as both a
  configured source and an installed projection, attribution discarded the
  invocation as ambiguous. Installed items are now treated as projections of
  their source, with symlink target, manifest path, or content hash breaking
  ties between sources.
- **Ambiguous invocations preserved.** Catalog-known names that cannot resolve
  uniquely are kept as unresolved events so usage is still counted and
  reconciliation can repair them later.
- **Global-scope reconciliation.** Usage rows without a `workspace_root` now
  re-resolve against the global catalog after catalog fixes.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
