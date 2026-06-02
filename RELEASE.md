# Release notes

Notable changes per release. Newest first. See [DEPLOYMENT.md](DEPLOYMENT.md)
for how releases are built and published.

## [Unreleased]

- fix(ui): the per-row "⋯" actions menu now appears when opened. `Button` was a
  plain function component (React-19 shadcn style) while the app runs React 18,
  so Radix's `asChild` trigger ref never reached the DOM node; floating-ui had
  no anchor and rendered the menu off-screen at its `translate(0, -200%)`
  placeholder. `Button` now uses `forwardRef`. Reverted the earlier hover/
  pointer-event workarounds, which were chasing symptoms.

## [0.2.0] — 2026-06-03

### Highlights

- A calmer, stickier Manager panel: your filters stay put as you move around the
  app, the matrix opens in tree view, and the per-row actions menu works again.

### Changes

- Capability filters (search, type, source, flat/tree view, collapsed folders)
  now persist across Manager/Suites/Config and Global/Workspace switches instead
  of resetting.
- The matrix defaults to tree view.
- New "Enabled only" filter narrows the list to capabilities enabled in at least
  one tool.
- Fixed the per-row "⋯" actions menu disappearing in the desktop WebView:
  Tailwind v4 gates `group-hover` behind `@media (hover: hover)`, which the macOS
  WebView did not match, so the trigger is now revealed with an ungated hover
  selector plus keyboard focus.
- Reduced the white background flash on first launch: the window starts hidden
  with a dark native background and is shown only once the WebView finishes
  loading.

### Migration

- None. No config, storage, or contract changes.

## [0.1.2] — 2026-06-02

- macOS releases are now code-signed with a Developer ID Application certificate and notarized via App Store Connect, so the DMG installs without Gatekeeper "Privacy & Security" prompts.

## [0.1.1] — 2026-06-01

- Empty-start scaffold: a first-run empty state bootstraps a bundled demo shared root (skills, agents, rules, hooks) in one click so a fresh install is usable immediately.
- Open files: a per-row actions menu opens a capability's original file in a configurable preferred editor (System default / VS Code / Cursor / custom), reveals it in Finder, and opens the file each enabled tool actually references — all through validated Rust commands using `tauri-plugin-opener`.
- Resolve "real file blocks this target" (`foreign_file`) conflicts: when you enable a capability whose tool target already holds a real file or folder, Apply now warns and, on explicit confirmation, deletes the blocking file/folder and projects. Without confirmation the conflict is still skipped — real files are never overwritten silently. The watcher and suite/workspace applies never take over.

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
