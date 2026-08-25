# Changelog

All notable changes to Agentic Hub are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
For narrative release notes, see [RELEASE.md](RELEASE.md).

## [Unreleased]

### Added

- **Grok Build tool adapter and usage tracing.** Opt-in Grok column projects skills, agents, rules, and commands into `~/.grok/`. Hooks with `"targets": ["grok"]` write Claude-style JSON under `~/.grok/hooks/`. When tracing is on, a managed tracer hook attributes Grok `promptId` turns, `read_file` SKILL.md reads, and qualified slash names (`/user:commit`).

## [0.16.2] — 2026-08-24

### Fixed

- **Codex (and Claude/Cursor) now track spaced slash skills and SKILL.md name aliases.** `/grill me` and `/Repo Research` were split on whitespace, so only `grill` / `Repo` reached the catalog and were dropped. Codex has no `UserPromptExpansion` or attachment fallback, so `grill-me` vanished on multi-skill turns. Hyphen-joined slash tokens and frontmatter `name` aliases (`grilling` → `grill-me`) now resolve to the folder skill.

### Added

- **Kiro usage tracing.** When Kiro is enabled, Agentic Hub installs a managed tracer hook under `~/.kiro/hooks/`, accepts `userPromptSubmit` payloads, and attributes global plus `.kiro/skills` repository skills.

## [0.16.1] — 2026-08-03

### Fixed

- **Codex, Claude, and Cursor now attribute skill usage identically.** Any name a
  tool held both as a configured source and as an installed projection produced
  two same-name catalog candidates, and attribution discarded the invocation as
  ambiguous — no `capability_id`, no dashboard row, no trace that the skill ran
  at all. It hit every tool through a different door: Claude's managed hard
  copies, unmanaged copies under a tool's skills root, stale manifest paths after
  a skill moved between source folders, and two configured sources sharing a leaf
  name. An installed item is now treated as its source's projection rather than a
  rival, and the projection's real origin (symlink target, manifest source path,
  or recorded content hash) breaks ties between sources. A sweep of 244 shared
  capability names across the three tools now resolves with no losses and no
  disagreements.
- **Ambiguous invocations are no longer thrown away.** A catalog-required
  reference whose name the catalog knows but cannot resolve uniquely is kept as
  an unresolved event instead of being dropped, so the usage is still counted and
  reconciliation can repair it later. Unknown names (`/health`, stray paths) are
  still ignored as noise.
- **Global-scope usage history is reconciled.** `reconcile_existing_usage`
  skipped every unresolved row without a `workspace_root`, so global skill
  invocations could never be repaired after a catalog fix. They now re-resolve
  against the global catalog.

## [0.16.0] — 2026-07-31

### Added

- **Guided stale-copy recovery.** Stale managed copies show an accessible
  warning control instead of a plain dot. Unowned stale projections offer
  **Refresh from source**, staging the projection for the normal ActionBar
  Apply step. Suite-owned stale projections offer **Re-sync current suite
  binding**, which reapplies the tool's live selected suite, base suite, and
  manual extras without changing the binding. Open source and Reveal target
  remain as manual fallbacks; broken and foreign states keep their existing
  explicit conflict handling.
- **ripgrep in the bundled CLI-tools catalog.** Resources' Tools pane now
  lists `rg`, checked with a plain `rg --version` probe (no authentication
  check) and linked to the official ripgrep installation docs.
- **Direct palette search shortcuts.** Three new global accelerators (default
  `Cmd+Alt+Ctrl+A` / `Cmd+Alt+Ctrl+S` / `Cmd+Alt+Ctrl+C`) jump straight into
  All resources, Skills, or Commands search, always showing and focusing the
  palette even if it's already open. The existing hub shortcut
  (`Cmd+Alt+A` by default) keeps its show/hide toggle behavior, and in-palette
  `Ctrl+1`–`7` mode shortcuts are unchanged. Config exposes all four
  shortcuts with validation against malformed or duplicate accelerators and
  an atomic reset-to-defaults action; a failed OS registration or settings
  write rolls back to the prior complete working set.

### Changed

- **Suites merged into the Manager rail.** The standalone Suites tab is gone;
  Global, Suites, and Workspaces now share one Manager table with the same
  filters, hierarchy, and row actions. Selecting a suite adds a tri-state
  **Included** column, and Save, Cancel, Delete, Set base, and Apply Suite
  behave as before. `#/suites` and the palette's Open Suites command still
  work as aliases into Manager's Suites mode.
- **Statistics opens on Today.** A new **Today** tab is the default view and
  shows the local-calendar-day usage table independent of the date range.
  **Usage overview** moved above **Most used** inside the renamed **Top
  usage** tab, and **Resource inventory** moved into its own **Inventory**
  tab so it stays reachable with tracing disabled. The date-range selector
  now appears only on Activity, Top usage, and Unused.
- **Resources rail reorders to Skills → Tools → Sessions.** Resources
  defaults to Skills when skills.sh is enabled and falls back to Tools
  otherwise, including when Skills becomes disabled while it was selected.

## [0.15.2] — 2026-07-22

### Added

- **App-wide color scheme.** Config now offers Light, Dark, and Follow system.
  The selected appearance applies immediately and stays synchronized across the
  main app, command palette, install window, and native chrome.
- **Install skills.sh skills into your library.** The install window gains a
  Workspace | Library scope toggle: Library stages a skill install, normalizes
  it into a configured Hub source root's `skills/` contract layout, and
  records it in that root's own `skills-lock.json` — so one install projects
  to every tool and project instead of living in a single throwaway project.
  Library-installed skills are badged in the Global Manager with the same
  "skills.sh" indicator and **Update via skills.sh** row action Workspace
  scope already had.

### Fixed

- **Statistics chart tooltips.** Recharts tooltip label, item, and hover cursor
  colors now follow the dark chart theme instead of defaulting to unreadable
  light-theme text.
- **Cursor `/skill` usage tracing.** Prompt-submit hooks now extract
  catalog-validated slash skills (`/name`), agent mentions (`@agent-*` /
  `/agent`), and agent markdown reads under `/agents/`, so Cursor no longer
  depends on a secondary Skill tool or `SKILL.md` attachment to count usage.
- **Delivery resilience.** Failed tracer curls spool under
  `~/.agentic-hub/usage/spool/` and drain on collector start and every 30s,
  instead of silently dropping events when the hub is briefly down.
- **Codex name precedence.** Same-named global and repository skills resolve
  workspace-first (aligned with Cursor), instead of staying unresolved when
  both scopes are present.
- Repository-local agents under documented tool agent dirs are included in the
  usage catalog so `/agent` and agent `Read` paths can resolve in-repo.

## [0.15.1] — 2026-07-20

### Added

- **Complete repository-local skill usage tracing.** Cursor, Claude Code, and
  Codex skill runs now resolve against each active repository as well as managed
  and tool-global skills, while retaining the repository identity in Manager
  counts and Statistics history.
- **Per-tool tracing diagnostics.** Config now reports whether each supported
  tool's hook is installed, its latest captured event, and resolved/unresolved
  event counts.

### Changed

- One user turn can now record every distinct referenced skill exactly once.
  Higher-confidence completion hooks upgrade prompt fallbacks instead of
  double-counting the same invocation.
- Usage storage schema v2 records global/workspace scope, canonical tildified
  workspace roots, capability-relative paths, privacy-safe invocation keys,
  and attribution confidence without storing prompts or tool arguments.

### Fixed

- Repository-local skills no longer remain unresolved merely because they are
  absent from Agentic Hub's saved global sources.
- Multi-skill prompts no longer discard all references, overlapping Claude hook
  signals no longer create duplicate counts, and generic slash commands or
  ordinary file attachments are no longer treated as skills.

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

## [0.14.0] — 2026-07-18

### Added

- **Statistics page sub-tabs.** Usage data is now organized into Overview,
  Activity, Top usage, and Unused tabs instead of one long scroll — switching
  tabs is faster since only the tab you're viewing renders its charts/tables.
- **Today's usage table.** The Overview tab now shows which capabilities
  you've used today, independent of whatever date range you have selected —
  handy for a quick end-of-day check without changing the range filter.

### Changed

- **Local usage tracing is now on by default.** New installs, and any settings
  file predating this option, start collecting local skill/tool usage
  automatically. It's local-only (SQLite, never leaves your machine) and can
  be turned off any time in Config → Local usage tracing. Settings files that
  already chose "off" explicitly keep that choice.
- Timestamps in Statistics and the Manager matrix now show in your local time
  instead of raw UTC.

### Fixed

- The "Today's usage" table now matches your machine's local calendar day
  instead of UTC, so it no longer drifts by a day around midnight.

## [0.13.1] — 2026-07-17

### Added

- **Local usage tracing health checks.** Collector status now uses an
  authenticated loopback probe rather than cached process state. The app checks
  an enabled collector hourly, retries recovery three times without touching
  tracer hooks, and sends one native/in-app restart prompt per continuous outage.

## [0.13.0] — 2026-07-17

### Added

- **App-wide color scheme.** Config now offers Light, Dark, and Follow system.
  The selected appearance applies immediately and stays synchronized across the
  main app, command palette, and install window. Fresh installs follow the OS;
  existing settings files keep the previous dark appearance until changed.

## [0.12.0] — 2026-07-15

### Added

- **Statistics page.** New top-level tab beside Config with a resource inventory
  section (total resources, per-kind counts, enabled tools, starred skills) plus
  overview cards, recharts bar charts (activity over time, by kind, by source
  tool), workspace breakdown, top-used table, and unused-installed pruning list.
  Usage aggregates are driven by `cmd_query_usage_dashboard` over
  `~/.agentic-hub/usage/trace.db` with 7d / 30d / 90d / all-time filters.

## [0.11.1] — 2026-07-08

### Added

- **First-class internal hooks projection.** Agentic Hub-owned hooks (starting with
  the local usage tracer) appear as read-only `Agentic Hub` rows in the Manager
  so their projection state is visible alongside user source-root hooks.
- **Settings-managed hook sync preservation.** Enabled internal hooks are merged
  into every hook sync for their target tool, so Manager applies, suite full
  resets, and watcher reconcile cannot remove tracer hooks that are absent from
  user hook lists.

### Changed

- **Usage tracer hook definitions live in `internal_hooks`.** The Tauri usage
  collector delegates tracer manifests and paths to `agentic-core`, removing
  duplicated hook metadata from the hub crate.
- **Manager and suite UI lock internal hook toggles.** Settings-managed rows stay
  in sync payloads but cannot be toggled from the Manager matrix, suite editor,
  or command palette; Config remains the control surface for usage tracing.

## [0.11.0] — 2026-07-07

### Added

- **Daily-active telemetry.** When usage telemetry is enabled, the app sends at
  most one `daily_active` ping per UTC day on first real engagement (window
  focus, palette summon, or dock reopen), with a stable anonymous `clientId` so
  unique active desktop installs can be counted in Aptabase.
- **Local usage tracing.** Opt-in, local-only tracing records explicit skill,
  agent, and command usage into `~/.agentic-hub/usage/trace.db`. The Manager
  matrix adds a **Usage** column with per-tool hover breakdowns.
- **Agent usage counts.** Agent specs are attributed when invoked via explicit
  slash commands, Claude `@agent-*` mentions, or a single Read of an agent
  markdown file under an `/agents/` path.
- **Command palette usage counts.** Copy and paste actions from the global
  command palette are recorded against command capabilities.

## [0.10.3] — 2026-07-04

### Added

- **Global Manager shows unmanaged tool installs.** The Global view merges Hub
  source-root resources with read-only rows discovered from enabled tools'
  native global folders (`~/.codex`, `~/.claude`, `~/.cursor`, and peers).
  Unmanaged rows are labeled by tool source, filterable via the Source filter,
  and support Open / Reveal without toggles, Apply, or sync. Duplicates already
  represented by Hub-managed projection state are suppressed.

## [0.10.2] — 2026-06-30

### Fixed

- **Kiro rules now project to AGENTS.md, not per-file steering copies.** Shared
  rules sync into the managed block at `~/.kiro/steering/AGENTS.md` (always
  included per [Kiro docs](https://kiro.dev/docs/steering/#agentsmd)); native
  `.kiro/steering/*.md` files with inclusion modes are left to the user.
  Workspace inventory also attributes project-root `AGENTS.md` to Kiro, Copilot,
  and Antigravity alongside Codex/Cursor.
- **Copilot workspace inventory includes root `AGENTS.md`.** Copilot reads both
  `.github/copilot-instructions.md` and workspace-root `AGENTS.md`.
- **Antigravity workspace inventory includes `.agents/AGENTS.md`.** Antigravity
  loads both repo-root and `.agents/AGENTS.md` per Gemini/Antigravity docs.

### Added

- **Config → Tools lists all five projection targets per tool.** Skills, agents,
  rules, hooks, and commands show their on-disk write paths with a reveal-in-Finder
  action so you can verify what the hub projects.

## [0.10.1] — 2026-06-30

### Fixed

- **Kiro projection used symlinks Kiro cannot load.** Kiro IDE ignores symlinks
  under `~/.kiro/skills/`, `~/.kiro/agents/`, and `~/.kiro/steering/` ([#6401](https://github.com/kirodotdev/Kiro/issues/6401)). Skills now use managed copy with flat layout (same strategy as Claude Code skills); agents and steering rules hard-copy too.
- **Copilot and Antigravity skills used nested paths their loaders never scan.**
  Copilot and Antigravity skill loaders are non-recursive (top-level only).
  Copilot skills now use flat layout (symlinks still work); Antigravity skills
  use managed copy with flat layout because Antigravity ignores symlinks
  ([#633](https://github.com/vercel-labs/skills/issues/633)).
- **Antigravity default skills path was wrong.** The hub defaulted to
  `~/.gemini/skills`; Antigravity reads `~/.gemini/config/skills`. New installs
  use the correct path; existing configs with the legacy default migrate once on
  launch (`Settings::migrate_antigravity_skills_path`).

## [0.10.0] — 2026-06-30

### Added

- **Kiro tool adapter.** Project shared skills, agents, steering rules, and hooks
  into `~/.kiro/` when enabled in Config (off by default). Hooks require
  `"targets": ["kiro"]`. Workspace inventory scans project `.kiro/` dirs.
- **GitHub Copilot tool adapter.** Project skills, custom agents (`.agent.md`),
  instruction rules (`.instructions.md`), and hooks into `~/.copilot/` when
  enabled (off by default). Hooks require `"targets": ["copilot"]`. Workspace
  inventory scans `.github/` Copilot dirs.
- **Google Antigravity tool adapter.** Project skills, rules (managed block in
  `~/.gemini/AGENTS.md`), and hooks (`~/.gemini/config/hooks.json`) when enabled
  (off by default). Agents and commands are unsupported. Hooks require
  `"targets": ["antigravity"]`. Workspace inventory scans `.agents/` dirs.

## [0.9.5] — 2026-06-26

### Added

- **Paste into focused app (macOS).** Opt in from Config → **Paste into focused
  app**. When enabled, choosing a command in the palette copies its body *and*
  pastes it into the app you were using (Alfred-style) — summon the palette,
  pick a slash command, hit Enter, and the prompt lands in your chat without
  a manual Cmd+V. Requires Accessibility permission for Agentic Hub in System
  Settings → Privacy & Security → Accessibility; the palette shows a toast if
  permission is missing. Off by default; clipboard-only behavior is unchanged
  when the toggle is off.

## [0.9.4] — 2026-06-26

### Added

- **Inline per-tool toggle in the command palette.** Selecting a skill, agent,
  rule, or hook now drills into a sub-panel that lists your enabled tools (Codex,
  Claude, Cursor…) with live on/off state — toggle a tool to enable or disable
  the capability for it **immediately**, no Manager round-trip or Apply step. The
  panel stays open for quick multi-tool edits and adds **Enable/Disable for all
  tools**, **Open in editor**, and **Reveal in Finder** rows; suite-managed cells
  are locked. **Alt+Enter / Alt+Click** still opens the source file. Each toggle
  reuses the manager's `plan → apply → syncRules → syncHooks` pipeline with the
  complete desired map (so other enabled rules/hooks are never dropped) and
  refreshes the main window's matrix. See
  [docs/features/command-palette.md](docs/features/command-palette.md).
- **Suite apply preserve mode:** Applying a suite previews manually enabled
  capabilities outside the effective suite and lets you remove or keep them.
- **Main window state persistence.** The hub remembers window size, position,
  and maximized state across launches.

### Changed

- **Suite apply manual extras:** Switching suites now tracks manually added
  capabilities separately from the previous suite's owned items. Preview shows
  only true manual extras; "Keep manually added" applies
  `new_suite ∪ base ∪ manual_extras` and persists them on the binding.
- **Command palette polish.** Keyboard navigation scrolls the selected row into
  view; the palette dismisses cleanly after suite apply confirmation.

## [0.9.3] — 2026-06-24

### Added

- **In-app auto-update.** Agentic Hub now updates itself with Tauri's built-in
  updater. It checks an R2-hosted feed (`latest.json`) on launch, on each app
  re-open, and weekly — throttled to once per 7 days — and offers a one-click
  **Install & Relaunch** when a newer minisign-signed release is available. An
  **App ▸ Check for Updates…** menu item forces an immediate check. The release
  workflow signs the updater bundle and syncs the `.dmg`, `.app.tar.gz`, and
  `latest.json` to Cloudflare R2 automatically on tag. See
  [DEPLOYMENT.md](DEPLOYMENT.md) (Auto-update section).

## [0.9.2] — 2026-06-22

### Added

- **Update skills.sh skills from the workspace inventory.** Workspace scope now
  reads a project's `skills-lock.json` and marks every skill the skills.sh CLI
  manages with a `skills.sh` badge. Each marked row gets an **Update via
  skills.sh** action that opens the existing install window in a focused update
  mode and runs `npx skills update <name> --project --yes` with live streaming,
  Cancel, and an automatic re-scan when it finishes. This is the second
  user-initiated workspace write (after install); scanning stays read-only and
  tolerant — a missing or malformed lock simply marks nothing. See
  [docs/features/skills-sh-integration.md](docs/features/skills-sh-integration.md)
  and [docs/tech/modules/skill-sources.md](docs/tech/modules/skill-sources.md).

## [0.9.1] — 2026-06-16

### Fixed

- **Install Skills window footer overlap.** On a short window the live output
  console kept a fixed minimum height and spilled over the Close / Cancel
  buttons. The console `<pre>` now shrinks to `min-h-0` and scrolls internally,
  and the footer is `shrink-0`, so the buttons always stay below the output.
- **Suite Manager includes hooks and commands.** The suite editor and capability
  checklist now list all five kinds (skills, agents, rules, hooks, commands) so
  suites can define and apply complete capability sets. The apply pipeline
  already projected hooks and commands; the UI had filtered them out.

## [0.9.0] — 2026-06-16

### Added

- **Tools preflight in the Resources panel.** The Resources tab is now a
  two-pane view: a left rail switches between **Tools** (always available) and
  **Skills** (the skills.sh browser, shown only when that source is enabled).
  The Tools pane is a preflight check for the command-line tools agents rely on
  — Node, Python, Homebrew, git, the GitHub/GitLab CLIs, the Claude/Codex/Cursor
  agent CLIs, and Vercel — showing each tool's install and (where applicable)
  auth status, with per-row and "Refresh all" re-checks and an Install link out
  to each tool's website. Tools come from a bundled JSON catalog
  (`resources/cli-tools/catalog.json`), mergeable with an optional user-local
  override via `settings.cliToolsPath` (remote/hot-update deferred). Status is
  probed in Rust with the resolved login `PATH` and a per-check timeout; probes
  run `program` + `args` directly (no shell). PATH resolution is now centralized
  in `agentic-core::shell_env`, shared with the skills installer. See
  [docs/tech/modules/cli-tools.md](docs/tech/modules/cli-tools.md).

### Security

- **`cliToolsPath` is config-file-only.** Because the tool catalog defines
  executables that get run, the untrusted WebView must not be able to set its
  path. `cmd_save_settings` now preserves the on-disk `cli_tools_path` and
  discards any value the renderer sends; the override is set only by
  hand-editing the settings file.
- **Login-`PATH` resolution is timeout-bounded and cached.** `shell_env`
  resolves the login shell `PATH` at most once (`OnceLock`) and kills the probe
  shell after a 5s timeout, so a hanging rc file can't stall tool checks.

## [0.8.3] — 2026-06-15

### Fixed

- **Agents in source subfolders never loaded in Cursor or Codex.** An agent
  organized under a nested path (e.g. `zoom/cto.md`) was projected to a matching
  subfolder (`~/.cursor/agents/zoom/cto.md`), but Cursor's and Codex's subagent
  loaders scan only the **top level** of their agents directory — nested files
  are ignored. Cursor and Codex agents now use `Layout::Flat`, collapsing the
  source path to its basename (`~/.cursor/agents/cto.md`,
  `~/.codex/agents/cto.toml`) so the loader discovers them. Claude is
  unchanged — its agent loader is recursive and keys on the `name` frontmatter,
  so Claude agents stay nested. Same-basename collisions from flattening resolve
  deterministically (lowest `item_id` wins; the rest become `skip_conflict`),
  and the upgrade self-heals: writing the flat copy prunes the old nested copy
  of the same item (`managed_copy::prune_other_paths_for_item`), including its
  now-empty folder. Only hub-managed copies are touched; user files are left
  alone.
- **Codex agent projection did nothing useful, in two compounding ways.**
  Enabling agents for Codex appeared to succeed in the matrix but Codex never
  picked the agent up. Two distinct root causes:
  1. *Wrong destination (older configs).* Configs persisted before v0.5.0
     retained the superseded Codex `agentsPath` default of `~/.agents/agents` —
     the OpenStandard-owned shared root — so projections collided there instead
     of landing where Codex reads subagents. A one-time migration
     (`Settings::migrate_codex_agents_path`, marker `codexAgentsPathMigrated`)
     rewrites that exact stale default to `~/.codex/agents` on launch; a
     deliberate custom path is left untouched.
  2. *Wrong format (all configs).* Even at the right path, the hub symlinked the
     raw markdown spec. Codex only loads `*.toml` subagent files (`name`,
     `description`, `developer_instructions`) from `~/.codex/agents/`, so the
     markdown link was ignored. Codex agents now use a new `CodexAgentToml`
     projection mode: the markdown source (YAML frontmatter + body) is rendered
     to a Codex subagent TOML and written as a managed copy at `<name>.toml`.
     Enabling self-heals by removing any superseded `.md` symlink the hub
     previously created; a user-authored `.md` at the same path is left
     untouched.

## [0.8.2] — 2026-06-15

### Added

- **Opt-in usage telemetry (Aptabase).** A new Config toggle
  (`Settings.telemetry.enabled`, off by default) enables anonymous lifecycle
  telemetry via `tauri-plugin-aptabase`. Only `app_started` / `app_exited` are
  sent, from Rust only, gated on consent at runtime — the WebView never calls
  out, and nothing is sent while disabled. Resolves the long-open telemetry
  question (decision D15).

### Changed

- **Refreshed app logo and bundled icons** across desktop, iOS, and Android icon
  sets.

### Fixed

- **Startup panic from the telemetry plugin.** The Aptabase plugin starts its
  background flush loop with a bare `tokio::spawn` during setup, which panicked
  at launch ("there is no reactor running") because Tauri does not enter a Tokio
  runtime on the main thread. `run()` now owns a multi-thread Tokio runtime and
  keeps its context entered for the app lifetime.

## [0.8.1] — 2026-06-12

### Fixed

- **Suites no longer go empty on git sync.** `cmd_apply_suite` and
  `cmd_update_suite` no longer persist the in-memory `backfill_sources` source
  qualification. That opportunistic `store.put` rewrote the synced suites file
  with device-specific qualifiers on every apply/edit, so two machines diverged
  and a later `git pull` line-merged the multi-line `capabilities` arrays into an
  empty set (suite name kept, resources lost). Apply and palette "Apply suite…"
  are now read-only over the suites file.

### Added

- **One-level `.bak` backups.** `SuiteStore::write_file` and
  `SkillFavoritesStore::write` copy the prior good (non-empty) file to
  `<file>.bak` before the atomic `tmp`+`rename` (`paths::back_up_dotfile`), so an
  accidental clobber is recoverable without `git checkout`.
- **Live reload of synced state files.** The watcher subscribes to the resolved
  suites and skill-favorites files (single-file, NonRecursive) and emits
  `suite-store-changed` (kind `external`) and the new `skills-favorites-changed`
  event, so the Suites and Resources views reload after an external rewrite (a
  `git pull` on a custom path) instead of holding — and later re-saving — a
  stale snapshot.
- **Favorites cross-device parity.** The starred-skills file gets suites-level
  robustness (backups + live reload) so a custom `favoritesPath` inside a git
  repo can be shared across machines.
- **Filter starred skills as you type.** A local search box on the Resources page
  and in the install-into-project window narrows the starred list instantly
  (matching name, repo, owner, or slug). In the install window, filtering only
  changes what is shown — already-selected skills stay queued for install even
  when hidden, and a column "select all" applies to the visible rows.

### Changed

- **Watcher forced on once; toggle moved to Config.** New
  `Settings.watcherForceMigrated` marker drives a one-time `setup()` migration
  that flips any paused config back on, then respects later user pauses. The
  enable/pause control moved from the header to **Config ▸ Source watcher**.

## [0.8.0] — 2026-06-10

### Added

- **Layered command palette.** The palette root is now a sectioned hub of
  first-class commands — Search (all / skills / agents / rules / hooks /
  commands / suites), Go to (global / workspace), Navigate (Manager / Suites /
  Config), and Actions (Apply suite…, Pause/Resume watching). Typing at the
  root filters the hub rows only; resource results live inside their drilled-in
  mode, so a query targets exactly one slice. Cross-kind search is the explicit
  "Search all resources" mode.
- **Go to global.** A locate mode for shared resources: Enter surfaces the row
  in the Manager matrix (global scope) with the same scroll-and-highlight the
  workspace locate uses. The `hub-locate` payload is now scope-tagged.
- **Watching toggle in the palette.** Pause/Resume watching persists through
  `cmd_set_watcher_enabled` and syncs the main window's header via the new
  `hub-watcher-changed` event — without surfacing the main window.
- **Search-mode keyboard shortcuts.** Press Ctrl+1…Ctrl+7 while the palette is
  open to jump straight into each search mode (all resources, skills, agents,
  rules, hooks, commands, suites). Hub rows show matching `⌃1`…`⌃7` hints.

### Changed

- Every palette drill-in view (search modes, suite-tools) shows a breadcrumb;
  Backspace on an empty query steps back one level, and suite-tools returns to
  the suite search mode it was entered from.

## [0.7.0] — 2026-06-08

### Added

- **Commands — a fifth capability kind.** Slash-command prompts (Cursor / Claude
  Code / Codex style) now live as file-based, nested markdown under
  `<root>/commands/**/*.md` and flow through the same scan → inspect → plan →
  apply pipeline. They project into each tool's commands directory — symlink for
  **Cursor** (`~/.cursor/commands`), **Codex** (`~/.codex/prompts`), and
  **OpenStandard** (`~/.agents/commands`); a managed copy for **Claude**
  (`~/.claude/commands`, whose loader does not follow symlinks). OpenClaw is
  unsupported. Workspace scope reports commands a project already has (read-only).
- **Palette copy / edit for commands.** The command palette gains a dedicated
  command provider: **Enter copies the command body to the clipboard** (for
  standalone paste), **Alt+Enter opens the source file** for editing. Reads go
  through the allowlist-gated `cmd_read_capability_body`; clipboard writes use
  `tauri-plugin-clipboard-manager`.
- **Demo scaffold seeds commands.** First-run scaffold now ships
  `commands/review/code-review.md` and `commands/git/commit.md`.

### Notes

- Configs written before this release load unchanged — the new `commandsPath`
  field defaults via serde, and the adapter falls back to the per-tool default so
  commands project for existing users without re-saving settings.

## [0.6.2] — 2026-06-06

### Added

- **OpenStandard tool.** A fifth tool adapter, `openstandard`, owns the
  open-standard `~/.agents` root and projects skills, agents, rules, and hooks
  there (`~/.agents/{skills,agents,rules}`, `~/.agents/AGENTS.md`,
  `~/.agents/hooks.json`). Skills/agents symlink, rules write a managed block in
  `~/.agents/AGENTS.md`, hooks use the JSON section. It is enabled by default and
  global-only (like OpenClaw, it has no workspace-scope inventory). Configs
  written before this tool existed load unchanged — the missing `openstandard`
  block is injected via a serde default.

### Changed

- **Codex is now self-contained under `~/.codex`.** The Codex `skillsPath`
  default moved from `~/.agents/skills` to `~/.codex/skills`; the open-standard
  `~/.agents` root is now owned by the new OpenStandard tool. Existing configs
  keep their persisted Codex `skillsPath` value (no destructive migration); only
  fresh installs and configs that omit the field pick up the new default.

## [0.6.1] — 2026-06-04

### Added

- **A dedicated, live-streaming install window.** Installing starred skills into
  a workspace now opens its own `install` window (mirroring the `palette`
  window) instead of a modal dialog. It hosts the **skill × tool matrix**,
  streams the `npx skills add` stdout/stderr **live** into an auto-scrolling
  console over a Tauri `Channel<SkillInstallEvent>`, and exposes a **Cancel**
  button that kills the in-flight child (closing the window does too). New IPC:
  `cmd_open_install_window`, `cmd_take_install_context`,
  `cmd_install_skill_stream`, `cmd_cancel_install`; new `install.json`
  capability; new exported types `SkillInstallEvent` and `InstallContext`. Core
  gains a pure `skills_install_command()` builder so the shell layer only does IO.
- **Scope-aware filter reset.** `resetScopedFilters()` on the manager-filters
  store clears the scope-specific filters (`source`, `enabledOnly`, `collapsed`,
  `locateId`) while preserving the universal `query`/`kind`/`view`.

### Changed

- **Global and Workspace are now one unified rail.** The header scope `Select` is
  gone. The Manager renders a single left `ScopeRail` with **Global pinned at the
  top** and the remembered workspaces below; `ManagerView` replaces
  `WorkspaceView` and serves both scopes. The rail is sticky and scrolls
  internally. Switching scope kind (global ↔ workspace) resets the scope-specific
  filters so a `source` selected in one scope can't blank the matrix in the other.
- **Workspace install is a batch matrix behind a floating action button.** A
  round **+** FAB in workspace scope opens the install window; tick any
  combination of starred skills and target tools (a column header toggles a tool
  across all skills) and install them sequentially — partial-tolerant, so a single
  failure never aborts the rest, and each is cancellable mid-run.
- **Install state left the skills store.** Selection and streaming now live in the
  install window; the skills store drops `installMany`/`installLog` and the
  single-skill `install`, and the blocking `cmd_install_skill` command is removed
  in favor of the streaming command.

### Fixed

- **Skill installs failing with "No such file or directory (os error 2)".** The
  provider read the user's `PATH` from a *non-interactive* login shell
  (`zsh -lc`), which sources `.zprofile`/`.zlogin` but **skips `.zshrc`** — where
  nvm/fnm/Homebrew almost always export `PATH` — so `npx` wasn't found. It now
  asks an **interactive** login shell (`$SHELL` first, then zsh/bash/sh, via
  `-ilc`) and frames the printed `PATH` with a sentinel so rc-file chatter can't
  corrupt it. A spawn that still can't find `npx` now returns a typed
  `SkillCliMissing` error (code `skill_cli_missing`) with an actionable hint
  instead of the raw OS error.
- **The Manager toolbar now lines up with the matrix.** The search bar and
  filters along the top edge stretch to match the table width, so the left and
  right edges stay aligned instead of drifting apart on wider inventories.
- **The flat / tree view buttons show which one is active.** Selecting a layout
  now highlights its button (a solid indigo pill); previously the active state
  was swallowed and both buttons looked the same.
- **Skill search moved into a focused modal.** On the Resources page, "Search
  skills.sh…" opens a dedicated dialog with its own scrolling result list, and
  your starred skills now fill the whole page instead of sharing the space with
  the search box.

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
