# Release notes

Notable changes per release. Newest first. See [DEPLOYMENT.md](DEPLOYMENT.md)
for how releases are built and published.

## [Unreleased]

- Empty-start scaffold: a first-run empty state bootstraps a bundled demo shared root (skills, agents, rules, hooks) in one click so a fresh install is usable immediately.
- Open files: a per-row actions menu opens a capability's original file in a configurable preferred editor (System default / VS Code / Cursor / custom), reveals it in Finder, and opens the file each enabled tool actually references — all through validated Rust commands using `tauri-plugin-opener`.

## [0.1.0] — 2026-05-31

First public build. Agentic Hub manages shared agentic capabilities — skills,
agents, rules, and hooks — across Codex, Claude Code, Cursor, and OpenClaw from
one window. The Rust core owns every filesystem mutation; the UI is a thin view
over typed IPC.

### Highlights

- **One window for every tool.** Scan a shared root once, then enable or disable
  each capability per tool from a single matrix — flat list or hierarchical tree,
  with search and filters by source and by type (skills / agents / rules / hooks).
- **Plan-then-apply, never destructive.** Changes are staged, planned from disk,
  and applied explicitly. Real files are never overwritten; conflicts surface as
  skips, never silent clobbers.
- **Suites.** Save named capability presets and apply a whole suite to a tool in
  one action, with a dedicated Suite Manager tab.
- **Workspaces.** Project a suite into a per-project `.agentic-hub/` folder as a
  self-contained hard copy.
- **Source Watcher.** Watches the configured source roots and, on any change
  (e.g. after a `git pull`), auto-reconciles every enabled tool's projections and
  re-patches the active workspace, then live-refreshes the UI. New files beside an
  enabled sibling auto-enable; foreign files/links are never taken over. Toggle it
  from the header (**Watch / Paused**); recover from Config ▸ **Rescan & resync
  everything**.

### Added

- Multi-source roots with priority and first-wins collision handling;
  `__archived__` folders are ignored.
- Configurable per-tool target paths, an optional custom suite-store path, and a
  tool-enablement panel (Codex / Claude / Cursor on by default, OpenClaw hidden).
- macOS universal release pipeline: GitHub Actions builds a signed-later `.dmg`
  on `v*` tags. See [DEPLOYMENT.md](DEPLOYMENT.md).
- Docs: [features/source-watcher.md](docs/features/source-watcher.md),
  [tech/modules/watcher.md](docs/tech/modules/watcher.md), and the full index at
  [docs/README.md](docs/README.md).

### Known issues

- macOS builds are **unsigned** this version: first launch needs right-click ▸
  Open (or System Settings ▸ Privacy & Security ▸ Open Anyway). Signing and
  notarization are tracked for a later release.
- Windows and Linux bundles are not produced yet.
