# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.15.0] — 2026-07-19

### Added

- **Session Explorer.** A new Sessions pane in the Resources rail browses
  your local Codex, Claude Code, and Cursor coding-agent session history —
  a filterable list plus a read-only, on-demand transcript view. Nothing is
  uploaded; content is read live from each tool's own files, defaulting to
  Today (rolling 24 hours) so opening the pane stays fast.
- **Copy as Markdown.** The Sessions pane's transcript view can copy an
  entire session (title, metadata, and every message) to the clipboard as
  one Markdown document, ready to paste elsewhere.

### Fixed

- The main window's Tauri capability now grants clipboard write access
  (previously scoped to the command palette window only), so Copy as
  Markdown can actually write to the clipboard.

### Known Issues

- The `r2.dev` feed is edge-cached, so a freshly published release can take up
  to a minute to appear to update checks.
