# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.1] — 2026-06-16

### Highlights

- **Suites now cover hooks and commands.** The Suite Manager capability tree
  lists all five capability kinds — skills, agents, rules, hooks, and commands —
  so a suite can capture and apply a complete tool configuration, not just the
  original three kinds. Apply semantics are unchanged: a full reset through the
  existing plan/apply, rule-sync, and hook-sync pipeline.

### Migration

- **None.** Existing suite files load unchanged; hooks and commands can be added
  to suites from the editor or will apply when already present in saved refs.
