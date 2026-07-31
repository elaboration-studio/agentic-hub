# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.16.0] — 2026-07-31

### Highlights

- **Guided recovery for stale managed copies** — refresh from source or
  re-sync a suite binding, without force or manual file surgery.
- **Suites unified into the Manager** — one table vocabulary for Global,
  Suites, and Workspaces.
- **Statistics opens on Today** — today's usage leads, everything else moved
  behind clearer tabs.
- **Direct palette shortcuts** — jump straight to All resources, Skills, or
  Commands search from anywhere.

### Added

- **Guided stale-copy recovery.** Stale managed copies show an accessible
  warning control instead of a plain dot. Unowned stale projections offer
  **Refresh from source**; suite-owned stale projections offer **Re-sync
  current suite binding**, reapplying the tool's live selected suite, base
  suite, and manual extras without changing the binding. Open source and
  Reveal target remain as manual fallbacks.
- **ripgrep in the bundled CLI-tools catalog.** Resources' Tools pane lists
  `rg`, checked with a plain `rg --version` probe and linked to the official
  installation docs.
- **Direct palette search shortcuts.** Three new global accelerators
  (default `Cmd+Alt+Ctrl+A` / `+S` / `+C`) jump straight into All resources,
  Skills, or Commands search, always showing and focusing the palette even
  if it's already open. The hub shortcut (`Cmd+Alt+A`) keeps its show/hide
  toggle, and in-palette `Ctrl+1`–`7` mode shortcuts are unchanged. Config
  validates all four accelerators against malformed or duplicate input and
  rolls back atomically if OS registration or the settings write fails.

### Changed

- **Suites merged into the Manager rail.** The standalone Suites tab is
  gone; Global, Suites, and Workspaces now share one Manager table with the
  same filters, hierarchy, and row actions, plus a tri-state **Included**
  column when a suite is selected. `#/suites` and the palette's Open Suites
  command still work as aliases into Manager's Suites mode.
- **Statistics opens on Today.** A new **Today** tab is the default and
  shows the local-calendar-day usage table independent of the date range.
  **Usage overview** moved above **Most used** inside the renamed **Top
  usage** tab, and **Resource inventory** moved into its own **Inventory**
  tab so it stays reachable with tracing disabled.
- **Resources rail reorders to Skills → Tools → Sessions**, defaulting to
  Skills when skills.sh is enabled and falling back to Tools otherwise.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
