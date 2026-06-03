# Changelog

All notable changes to Agentic Hub are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
For narrative release notes, see [RELEASE.md](RELEASE.md).

## [0.3.0] — 2026-06-03

### Added

- **Command palette.** An Alfred-style floating panel, summoned by a
  configurable global shortcut (default `Cmd+Alt+A`), searches your resources
  and opens the original file in your editor. It dismisses on blur or `Esc`.
- **Native macOS menus.** Standard App / Edit / View / Window menus. "Settings…"
  (`Cmd+,`) jumps to Config and "Command Palette" toggles the panel; `Cmd+Q`
  remains the hard exit.
- **Command-provider registry.** An extensible registry backs the palette,
  shipping navigation and suite-apply commands alongside flat resource search.
  Resource search opens files through the existing opener allowlist.
- **Palette suite apply (two-level).** Search a suite, drill into a suite-tools
  view (`‹ <suite>` breadcrumb, Backspace-to-back), and apply it to one tool as
  a full reset (clean + replace).
- **Suite↔tool bindings.** A new `~/.agentic-hub/suite-bindings.json` records
  which suite is applied to each tool. `cmd_apply_suite` records the binding,
  `cmd_update_suite` re-applies the new capability set to every bound tool
  (serialized via the reconcile guard, emits `sources-changed`), and
  `cmd_delete_suite` drops the bindings without touching tool projections.
- **Configurable palette shortcut.** Config gains a Command Palette panel to
  edit the global shortcut; saving re-registers it live. The new
  `Settings.paletteShortcut` field defaults to `Cmd+Alt+A` for existing configs.
- **Source-aware suites (cross-device portability).** Every scanned
  `CapabilityItem` now carries a portable `source` identity (`SourceRef`:
  home-relative path + folder name), and suite entries are source-qualified
  (`SuiteCapabilityRef { cap, source }`). A suite synced across devices resolves
  per-source: a reference whose source is absent on the current machine is
  skipped and preserved (counted as `ApplySuiteResult.skippedAbsentSource`),
  never deleted and never mis-resolved onto a same-named capability from a
  different source. `SuiteValidationResult` gains `absentIds`.

### Changed

- **Suite capabilities are objects, not bare strings.**
  `SuiteDefinition.capabilities` is now `SuiteCapabilityRef[]`. Legacy
  bare-string suite files load unchanged and upgrade in place on the next save
  (non-breaking); apply/update opportunistically backfill a source for
  unqualified refs that resolve to exactly one scanned item.

## [0.2.1] — 2026-06-03

### Fixed

- **The per-row "⋯" actions menu now appears when opened.** `Button` was a
  React-19-style component while the app runs React 18, so Radix could not
  attach its `asChild` trigger ref to the DOM node; the menu opened but rendered
  off-screen with no anchor. Buttons now forward their ref. This supersedes the
  0.2.0 hover-styling explanation below, which was a misdiagnosis of the same
  symptom.

## [0.2.0] — 2026-06-03

### Added

- **"Enabled only" filter.** A checkbox in the capability matrix narrows the list
  to capabilities enabled in at least one tool.

### Changed

- **Filters now persist across views.** Search, type/source filters, the
  flat/tree view, the enabled-only toggle, and collapsed folders survive
  switching between Manager, Suites, and Config and between Global and Workspace
  scope, instead of resetting each time.
- **The matrix opens in tree view by default** instead of the flat list.
- **Smoother first launch.** The window starts hidden with a dark background and
  appears only once the WebView has finished rendering, removing the white flash
  on startup.

### Fixed

- **Restored the per-row "⋯" actions menu.** It had stopped appearing in the
  desktop app because Tailwind v4 gates hover styles behind `@media (hover:
  hover)`, which the macOS WebView does not match; the menu now reveals on row
  hover (and keyboard focus) again.

[0.2.0]: https://github.com/elaboration-studio/agentic-hub/releases/tag/v0.2.0

## [0.1.2] — 2026-06-02

### Changed

- **Signed and notarized macOS releases.** CI now signs the universal DMG with a
  Developer ID Application certificate and notarizes it via App Store Connect, so
  installs pass Gatekeeper without manual Privacy & Security approval.

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

[0.1.2]: https://github.com/elaboration-studio/agentic-hub/releases/tag/v0.1.2
[0.1.0]: https://github.com/elaboration-studio/agentic-hub/releases/tag/v0.1.0
