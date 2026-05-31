# Changelog

All notable changes to Agentic Hub are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
For narrative release notes, see [RELEASE.md](RELEASE.md).

## [0.1.1] — 2026-06-01

### Added

- **Empty-start scaffold.** A first-run empty state bootstraps a bundled demo
  shared root (skills, agents, rules, hooks) in one click, so a fresh install is
  usable immediately.
- **Open files.** A per-row actions menu opens a capability's original file in a
  configurable preferred editor (System default / VS Code / Cursor / custom),
  reveals it in Finder, and opens the file each enabled tool actually
  references — all through validated Rust commands using `tauri-plugin-opener`.
- **Configurable editor preference** in the Config page.

### Changed

- **Foreign-file conflicts are resolvable.** Enabling a capability whose tool
  target already holds a real file or folder now warns and, on explicit
  confirmation, deletes the blocking file/folder and projects. Without
  confirmation the conflict is still skipped — real files are never overwritten
  silently. The watcher and suite/workspace applies never take over.

[0.1.1]: https://github.com/elaboration-studio/agentic-hub/releases/tag/v0.1.1

## [0.1.0] — 2026-05-31

First public build — a Tauri 2.x desktop app that manages shared agentic
capabilities (skills, agents, rules, hooks) across Codex, Claude Code, Cursor,
and OpenClaw from one window.

### Added

- **Capability matrix.** Scan a shared root and enable/disable each capability
  per tool from one view — flat list or hierarchical tree, with search and
  filters by source and by type (skills / agents / rules / hooks).
- **Plan-then-apply engine.** Changes are staged, planned from disk, and applied
  explicitly. Real files are never overwritten; conflicts surface as skips, never
  silent clobbers.
- **Suites.** Save named capability presets and apply a whole suite to a tool in
  one action, managed from a dedicated Suite Manager tab.
- **Workspaces.** Project a suite into a per-project `.agentic-hub/` folder as a
  self-contained hard copy, with a manifest cleanup cycle.
- **Source Watcher.** Watches the configured source roots and, on any change
  (e.g. after a `git pull`), auto-reconciles every enabled tool's projections and
  re-patches the active workspace, then live-refreshes the UI. New files beside an
  enabled sibling auto-enable; foreign files and links are never taken over.
  Toggle from the header (**Watch / Paused**, `watcherEnabled`, default on);
  recover via Config ▸ **Rescan & resync everything**.
- **Multi-source roots** with priority and first-wins collision handling;
  `__archived__` folders are ignored.
- **Configuration page** for per-tool target paths, an optional custom
  suite-store path, and tool enablement (Codex / Claude / Cursor on by default,
  OpenClaw hidden).
- **macOS release pipeline.** GitHub Actions builds a universal `.dmg` on `v*`
  tags and publishes a draft GitHub Release. See [DEPLOYMENT.md](DEPLOYMENT.md).

### Known issues

- macOS builds are **unsigned** this version: first launch needs right-click ▸
  Open. Signing and notarization are planned for a later release.
- Windows and Linux bundles are not produced yet.

[0.1.0]: https://github.com/elaboration-studio/agentic-hub/releases/tag/v0.1.0
