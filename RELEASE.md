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
