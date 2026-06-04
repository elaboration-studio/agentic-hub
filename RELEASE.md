# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.5.0] — 2026-06-04

### Highlights

- Workspace scope is now a **read-only audit**. Add a project to the left rail
  and the Manager matrix shows exactly which skills, agents, and rules each tool
  (Cursor, Claude, Codex) already has in that project — live-updated as the
  project's own tool dirs change. The hub never writes into a workspace.
- The old "apply a suite into a workspace" flow is gone for good. Capabilities
  are still written only through the global projection engine, where the
  plan-then-apply safety net lives.
- The command palette can now **search across every remembered workspace** and
  **locate** an item — search a project by name, hit Enter, and the Manager
  jumps to workspace scope and highlights that row in the matrix (no file is
  opened). The palette also renders as a clean floating card on macOS again.
- **Skills.sh as a pluggable skill source (opt-in).** Enable it in Config, then
  search [skills.sh](https://skills.sh) and **star** skills locally. From
  Workspace scope, **Install skill…** runs the source CLI into the active
  project; the read-only inventory re-scans to show what landed. Built behind a
  provider seam for future registries.

### Changes

- New `cmd_scan_workspace` (backed by `agentic-core::workspace_inventory`) walks
  a project's `.cursor` / `.claude` / `.agents` skills + agents, `.cursor/rules`,
  and `AGENTS.md` / `CLAUDE.md`, dedupes resources across tools, and returns a
  `WorkspaceInventory { items, states, errors }` with present-only `enabled`
  states. The global Manager matrix renders it read-only (static cells, inert
  aggregates).
- The filesystem watcher now also subscribes to the active workspace's tool
  dirs and emits a new `workspace-changed` event so the inventory live-refreshes.
  Picking, activating, or removing a workspace restarts the watcher.
- The command palette searches every remembered workspace's inventory (matched
  by project name + item name / path / source) and, on Enter, emits a new
  `hub-locate` event that surfaces the item in the Manager's workspace matrix.
  Palette window rendering on macOS is fixed: the `NSPanel` re-applies
  transparency and drops its native shadow after the style-mask change, sizes to
  its content, and no longer collapses the result list to one row.

- New opt-in **skills.sh source**. Config gains a Skills.sh panel (enable toggle,
  starred-file override, CLI check). A **Skills** tab (shown only when enabled)
  searches skills.sh and stars favorites to
  `~/.agentic-hub/skills-favorites.json`. Search needs **no API key** — it uses
  the keyless public index (`https://skills.sh/api/search`, the same endpoint the
  `skills` CLI uses), routed through Rust (`cmd_search_skills`) since that
  endpoint sends no CORS header. Workspace scope gains an **Install skill…**
  action backed by `cmd_install_skill`, which runs `npx skills add <owner/repo>`
  via a controlled subprocess (no `tauri-plugin-shell`) into the active project
  and re-scans the read-only inventory. New `agentic-core` modules `skill_source`
  (provider seam + `SkillsShProvider`) and `skill_favorites`, plus `SkillsConfig`
  on `Settings`.

### Removed (breaking)

- `cmd_apply_workspace_patch`, the `agentic-core::workspace_patch` module, the
  `WorkspacePatchResult` / `WorkspaceApply` types, `WorkspaceTarget.lastApplied`
  + `record_apply`, the `<ws>/.agentic-hub/workspace-patch.json` manifest, and
  the `workspace-apply-progress` event. Any caller of the workspace-apply IPC
  must migrate to the read-only inventory.

### Migration

- No on-disk migration needed. Old `<ws>/.agentic-hub/workspace-patch.json`
  manifests are simply ignored — workspace scope no longer reads or writes them.
  Existing workspace targets in `~/.agentic-hub/state.json` load unchanged; the
  obsolete `lastApplied` field is dropped on the next write.
