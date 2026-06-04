# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.6.1] — 2026-06-04

### Highlights

- **Workspace install, refined.** Installing starred skills into a project is now
  faster and roomier:
  - **A floating action button.** The cramped "Install skill…" toolbar button is
    gone; a round **+** button in the bottom-right corner opens the installer and
    stays reachable while the inventory matrix scrolls.
  - **Install several skills at once.** The picker is now a **skill × tool
    matrix** — tick the tools you want for each starred skill (with a column
    header to select a tool across all skills) and install the whole batch in one
    click. One failed install never aborts the rest.
- **The workspace list stays put.** The left rail of workspaces is now pinned to
  the side and scrolls on its own, so picking a project no longer means scrolling
  past a long inventory to get back to the list.

### Migration

- None. UX-only enhancements; settings, favorites, and workspace state load
  unchanged.
