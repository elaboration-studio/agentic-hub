# Feature: Workspace Suite Sync

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [docs/features/suite-presets.md](./suite-presets.md), [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md), [ARCHITECTURE.workspace.md](../../ARCHITECTURE.workspace.md)
Related Docs: [docs/tech/modules/workspace-patch.md](../tech/modules/workspace-patch.md), [docs/tech/modules/rule-projection-sync.md](../tech/modules/rule-projection-sync.md)

## Why now

Suites already let users toggle a curated capability set into a tool's global home directory (`~/.codex`, `~/.claude`, `~/.cursor`). That covers personal defaults. It does not cover project-level workflows.

A user often wants:
- A different suite per repo — a backend service repo gets coding agents, a marketing repo gets writing agents.
- Skills and rules **co-located with the project** so collaborators on the same repo see the same agentic context.
- Hard-overwrite semantics that don't depend on symlinks, since many work repos sync over Dropbox / iCloud / Windows where symlinks are flaky.

Workspace Suite Sync turns the existing suite + manager into a per-project patch that hard-copies a chosen suite into a workspace directory.

## User story

As a user with multiple project repos, I want to apply a saved suite directly into a project folder so that Codex, Claude Code, and Cursor pick up project-scoped skills, agents, and rules — without touching my global home directory.

## Scope

### In scope

- "Workspace" sync scope, alternating with "Global" via the header toggle.
- Workspace target directory selector — pick once via Tauri folder dialog, remembered across sessions with LRU ordering.
- Per-tool workspace target paths for Codex, Claude Code, and Cursor.
- Suite Apply (full reset) into the chosen workspace using hard copy / managed markdown section.
- Manifest at `<ws>/.agentic-hub/workspace-patch.json` so the next apply cleans the prior payload before writing.
- Recent workspace list with LRU ordering (capped at 12) and a "Forget" action per entry.

### Out of scope

- Per-item toggling in workspace scope (suite is the only entry point).
- Workspace target for OpenClaw.
- Auto-discovery of the currently open workspace folder in any external editor.
- Drift detection or watch-mode for workspace files.
- A "Clear Workspace Patch" button (the manifest tracks state and gets overwritten on the next apply).

## Per-tool workspace targets

| Tool | Skills | Agents | Rules |
|------|--------|--------|-------|
| Codex | `<ws>/.agents/skills/` | `<ws>/.agents/agents/` | managed section in `<ws>/AGENTS.md` |
| Claude Code | `<ws>/.claude/skills/` | `<ws>/.claude/agents/` | managed section in `<ws>/CLAUDE.md` |
| Cursor | `<ws>/.cursor/skills/` | `<ws>/.cursor/agents/` | `<ws>/.cursor/rules/*` (raw file copy) |

Codex projects under `.agents/` rather than `.codex/` because OpenAI Codex's documented skill scan paths are `$CWD/.agents/skills` walking up to `$REPO_ROOT/.agents/skills` (and `$HOME/.agents/skills` for user scope). Codex agents follow the same `.agents/` root for consistency.

All projections use **hard overwrite**: prior files written by the manager are removed first, then the new payload is copied in. Codex and Claude rules write a managed `<!-- e-studio-agentic-rules:start -->`/`...:end -->` block, so unmanaged content in `AGENTS.md` / `CLAUDE.md` is preserved.

## Experience

```
Header
  Agentic Hub
  Scope: [ Global | Workspace ]
  Suite: [ coding-mode ▾ ]   [ Apply to Workspace ]
  Refresh   Settings

  --- Workspace mode only ---
  Tool tabs:  [Codex] [Claude] [Cursor]
  Workspace: [ ~/Code/foo ▾ ]   [ Choose Folder… ]   [ Forget ]
```

In workspace scope the inventory tree and per-item staging are hidden. The body shows a single "Workspace Patch Preview" panel with:

- Active workspace directory.
- Selected suite name and description.
- Resolved target paths for skills / agents / rules under the focused tool.
- Suite contents breakdown (counts and per-item list).
- Last apply summary (written / cleaned / stale / errors) once an apply has run.

### Apply flow

1. User clicks the **Workspace** scope toggle.
2. User picks a workspace folder via `Choose Folder…` (Tauri dialog) — saved to the LRU list, reusable across sessions.
3. User selects a suite from the dropdown.
4. User selects the focused tool (Codex / Claude / Cursor).
5. User clicks **Apply to Workspace**.
6. Confirmation dialog warns that the operation is a hard overwrite into the chosen path.
7. On confirm:
   - The prior manifest at `<ws>/.agentic-hub/workspace-patch.json` is read.
   - Prior payload is removed (files, dirs, and managed markdown block via sentinel cleanup).
   - Suite items are hard-copied into per-tool target paths.
   - For Codex/Claude rules, the managed section in `AGENTS.md` / `CLAUDE.md` is rewritten.
   - A new manifest is written atomically.
8. A result summary toast reports written / cleaned counts and any errors.

## Acceptance criteria

- [ ] Header has a Global / Workspace scope toggle and the Workspace tab is visually distinct when active
- [ ] Choosing a workspace folder via `Choose Folder…` persists across app restarts
- [ ] The workspace selector lists previously chosen folders, ordered by `lastUsedAt`, capped at 12, with a `Forget` action per entry
- [ ] Tool tabs in workspace scope are filtered to Codex, Claude Code, and Cursor only
- [ ] Applying a suite into a workspace produces files at the per-tool paths above and a manifest at `<ws>/.agentic-hub/workspace-patch.json`
- [ ] Re-applying a different suite removes the prior payload before writing the new one
- [ ] Applying with a different tool removes the previous tool's payload (manifest is single-tool)
- [ ] Codex/Claude rule application writes a managed marker block in `AGENTS.md` / `CLAUDE.md` and preserves unmanaged content
- [ ] The apply confirmation modal shows the workspace dir and suite name
- [ ] Applying refuses paths that resolve outside the chosen workspace
- [ ] Stale suite references are skipped and counted in the result summary
- [ ] Symlinks inside a shared skill directory are dereferenced and copied as their resolved file/dir contents (no symlinks cross the workspace boundary)
- [ ] Manifest writes are atomic via `.tmp` + rename

## Dependencies

- `agentic-core::suite_store` (from suite-presets feature)
- `agentic-core::adapter_registry::create_workspace_adapter`
- New `agentic-core::workspace_patch` module
- New `agentic-core::workspace_target_store`
- `tauri-plugin-dialog` for folder picker
- New IPC commands: `cmd_pick_workspace_dir`, `cmd_list_workspace_targets`, `cmd_forget_workspace_target`, `cmd_apply_workspace_patch` (see [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md))

## Delivery slices

| Slice | What ships |
|-------|-----------|
| V1: Target store + workspace adapter | `workspace_target_store`, `create_workspace_adapter`, scope toggle UI |
| V2: Apply pipeline | `workspace_patch::apply`, manifest read/write, cleanup pass, out-of-workspace guard |
| V3: Rule sync integration | Codex/Claude managed block in `<ws>/AGENTS.md`, `<ws>/CLAUDE.md`; sentinel cleanup |
| V4: Polish | Confirmation copy, result summary detail, stale-id counting, error reporting |

## Risks and edge cases

- **Workspace dir disappears between pick and apply** — refused at apply time with clear error
- **Workspace dir is on a different filesystem** — copy still works; `fs::rename` may fail for the atomic manifest write; fall back to direct write with a warning logged
- **User edits a copied file directly** — workspace mode has no drift detection; the next apply hard-overwrites
- **Nested workspaces** — applying to `<outer>` then to `<outer>/<inner>` may shadow files; canonical-path guard prevents writes outside the apply's own workspace; documented as user responsibility
- **Symlink cycle inside a shared skill dir** — bounded copy depth; cycle stops with an error recorded

## Metrics or signals

- Number of distinct workspaces used per user (signals fit)
- Frequency of workspace apply vs global apply
- Workspace apply error rate

## Open questions

- Should we add a "match my current terminal cwd" shortcut in the picker? Deferred — explicit choice for now
- Gitignore guidance: should the app offer to write a `.gitignore` entry on first apply? Current decision: no automatic mutation in v1; document the recommendation in install docs and as a tooltip
- Should we allow committing the manifest to the repo for team-shared project capability state? Allowed by default (no .gitignore mutation); user decides
