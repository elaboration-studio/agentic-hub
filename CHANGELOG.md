# Changelog

All notable changes to Agentic Hub are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
For narrative release notes, see [RELEASE.md](RELEASE.md).

## [0.6.0] — 2026-06-04

### Added

- **Skills.sh as a pluggable resource source (opt-in).** A `SkillsConfig` on
  `Settings` (off by default) enables a Config panel (CLI check, starred-file
  override) and a conditional **Resources** tab that searches skills.sh and
  stars favorites to `~/.agentic-hub/skills-favorites.json` (new
  `agentic-core::skill_favorites`). Search uses the **keyless** public index
  (`https://skills.sh/api/search` — the same endpoint the `skills` CLI uses, no
  API key), routed through Rust (`cmd_search_skills`, a blocking `reqwest` GET)
  because that endpoint sends no CORS header. Workspace scope gains **Install
  skill…** (`cmd_install_skill`): the one explicit, user-initiated workspace
  write. It runs `npx skills add <owner/repo>` via a controlled
  `std::process::Command` (validated ref, cwd = the remembered workspace,
  login-shell `PATH`, no `tauri-plugin-shell`), then re-scans the read-only
  inventory. Built behind a `SkillProvider` seam (`agentic-core::skill_source`)
  for future registries. New IPC: `cmd_skill_cli_check`, `cmd_search_skills`,
  `cmd_list_skill_favorites`, `cmd_add_skill_favorite`, `cmd_remove_skill_favorite`,
  `cmd_install_skill`; new error codes `invalid_skill_ref`, `unknown_provider`,
  `skill_search`, `install_failed`.

### Changed

- **The "Skills" tab is now "Resources."** skills.sh is the first of several
  planned public resource channels, so the tab name reflects the broader scope.
  The internal route key is unchanged.

### Fixed

- **External links on skill rows now open.** The "Open on skills.sh" and "Open
  on GitHub" buttons (and the Config skills.sh link) used plain anchors, which
  are a no-op inside the Tauri WebView. They now route through a new
  `cmd_open_url` command that opens the URL in the system browser after
  validating it is an `http`/`https` URL with a host
  (`agentic-core::open_targets::is_safe_external_url`). New error code
  `url_not_openable`.

## [0.5.0] — 2026-06-04

### Added

- **Read-only workspace inventory.** Workspace scope now audits a project
  instead of writing into it. A left rail lists remembered workspaces; selecting
  one runs the new `cmd_scan_workspace` (`agentic-core::workspace_inventory::scan_workspace`),
  which walks each workspace tool's own dirs (`.cursor`/`.claude`/`.agents`
  skills + agents, `.cursor/rules`, `AGENTS.md`/`CLAUDE.md`), dedupes resources
  across tools, and returns `WorkspaceInventory { items, states, errors }` with
  present-only `enabled` states. The global Manager matrix renders it read-only
  (static present cells, inert aggregates).
- **Live workspace refresh.** The filesystem watcher subscribes to the active
  workspace's tool dirs and emits a new `workspace-changed` event; the UI
  re-scans on change. Picking / activating / removing a workspace restarts the
  watcher so it tracks the new active dirs.
- **Palette workspace search & locate.** The command palette now searches the
  read-only inventory of every remembered workspace (matched by project name
  plus item name / path / source) and **locates** a hit in the Manager's
  workspace matrix — switching to workspace scope, activating the workspace, and
  scrolling to and highlighting the row instead of opening a file. Backed by a
  new `hub-locate` window event.

### Fixed

- **The command palette renders as a clean floating card on macOS.** The
  `NSPanel` now re-applies transparency and drops its native window shadow after
  the style-mask change, so the native background and border no longer bleed
  through the rounded card's corners. The transparent window sizes to its
  content (removing the "stacked layers" dead space), and the result list no
  longer collapses to a single visible row.

### Removed

- **Suite-apply-into-workspace (breaking).** The `cmd_apply_workspace_patch`
  command, the `agentic-core::workspace_patch` module, the `WorkspacePatchResult`
  / `WorkspaceApply` types, the `WorkspaceTarget.lastApplied` field +
  `record_apply`, the `<ws>/.agentic-hub/workspace-patch.json` manifest, and the
  `workspace-apply-progress` event are all gone. Workspace scope no longer writes
  anything; capabilities are still written only via the global projection engine.

### Migration

- No on-disk migration needed. Obsolete `workspace-patch.json` manifests are
  ignored. Workspace targets in `~/.agentic-hub/state.json` load unchanged; the
  dropped `lastApplied` field is removed on the next write.

## [0.4.0] — 2026-06-03

### Added

- **Palette suite apply (two-level).** Search a suite, drill into a suite-tools
  view (`‹ <suite>` breadcrumb, Backspace-to-back), and apply it to one tool as
  a full reset (clean + replace).
- **Suite↔tool bindings.** A new `~/.agentic-hub/suite-bindings.json` records
  which suite is applied to each tool. `cmd_apply_suite` records the binding,
  `cmd_update_suite` re-applies the new capability set to every bound tool
  (serialized via the reconcile guard, emits `sources-changed`), and
  `cmd_delete_suite` drops the bindings without touching tool projections.
- **Source-aware suites (cross-device portability).** Every scanned
  `CapabilityItem` now carries a portable `source` identity (`SourceRef`:
  home-relative path + folder name), and suite entries are source-qualified
  (`SuiteCapabilityRef { cap, source }`). A suite synced across devices resolves
  per-source: a reference whose source is absent on the current machine is
  skipped and preserved (counted as `ApplySuiteResult.skippedAbsentSource`),
  never deleted and never mis-resolved onto a same-named capability from a
  different source. `SuiteValidationResult` gains `absentIds`.
- **Base suite (global merge) + Manager suite-lock.** A suite can be marked
  base (portable `SuiteDefinition.isBase`; at most one, enforced by the store).
  Its capabilities union into every global apply via `merge_base_caps`, so its
  rules/skills are always present; the recorded binding stays the selected
  suite. `cmd_set_base_suite(id | null)` flips the flag and re-applies every
  bound tool, and editing the base re-syncs every binding. `cmd_suite_ownership`
  reports which suite owns each `(tool, item)` (`SuiteOwnership`, `fromBase`),
  and the Manager matrix locks those cells, naming the owning suite on hover.

### Changed

- **Suite capabilities are objects, not bare strings.**
  `SuiteDefinition.capabilities` is now `SuiteCapabilityRef[]`. Legacy
  bare-string suite files load unchanged and upgrade in place on the next save
  (non-breaking); apply/update opportunistically backfill a source for
  unqualified refs that resolve to exactly one scanned item.
- **Manager UX polish.** The filter bar and table header stay pinned while a long
  capability list scrolls. Suite-locked cells now read as a distinct indigo
  dashed lock with a not-allowed cursor (and still name the owning suite on
  hover), and an enabled cell's green highlight is clearer in both the flat and
  tree views.

### Fixed

- **The Manager refreshes after a suite apply or base-suite change.** Applying a
  suite and setting or clearing the base now emit `sources-changed`, so the
  matrix reloads and keeps cell state and suite locks in sync without a manual
  rescan.

## [0.3.0] — 2026-06-03

### Added

- **Command palette.** An Alfred-style floating panel, summoned by a
  configurable global shortcut (default `Cmd+Alt+A`), searches your resources
  and opens the original file in your editor. It dismisses on blur or `Esc`.
- **Native macOS menus.** Standard App / Edit / View / Window menus. "Settings…"
  (`Cmd+,`) jumps to Config and "Command Palette" toggles the panel; `Cmd+Q`
  remains the hard exit.
- **Command-provider registry.** An extensible registry backs the palette,
  shipping navigation commands alongside flat resource search. Resource search
  opens files through the existing opener allowlist.
- **Configurable palette shortcut.** Config gains a Command Palette panel to
  edit the global shortcut; saving re-registers it live. The new
  `Settings.paletteShortcut` field defaults to `Cmd+Alt+A` for existing configs.

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
