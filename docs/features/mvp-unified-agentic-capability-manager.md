# MVP Feature: Unified Agentic Capability Manager

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/suite-presets.md](./suite-presets.md), [docs/features/workspace-suite-sync.md](./workspace-suite-sync.md), [docs/features/agentic-demo-scaffold.md](./agentic-demo-scaffold.md), [docs/tech/modules/rule-projection-sync.md](../tech/modules/rule-projection-sync.md), [docs/tech/modules/claude-flat-skill-layout.md](../tech/modules/claude-flat-skill-layout.md)

## Why now

Power users of agentic AI tools collect skills, agents, and rules across Codex, Claude Code, Cursor, and OpenClaw. Each tool reads from a different location with different layout rules. Keeping a single shared library projected into all four tools by hand is error-prone and slow. The VS Code extension proved this is solvable inside one editor. Agentic Hub generalizes the solution into a standalone desktop app so the workflow works regardless of editor.

This MVP feature is the foundation. Everything else (suites, workspace patch, demo scaffold) builds on top.

## MVP position

> Let a power user see local `skills`, `agents`, and shared prompt `rules` from one shared root and safely enable or disable them for one agentic tool at a time, from a desktop window.

If Agentic Hub does not replace a bash-based switching workflow for at least one real use case after this MVP ships, the MVP failed.

## User story

As a power user managing multiple AI tools, I want a single desktop window that scans my shared agentic root, shows me what is enabled per tool, and lets me batch toggle enable/disable changes with safe filesystem semantics, so that I can stop maintaining symlinks by hand.

## Scope

### In scope

- Tauri 2.x main window: header (title, shared root, scope toggle, tool tabs, refresh, settings), body (capability list + inspector), footer (staged count, apply, clear)
- Global settings persisted at `~/.agentic-hub/config.json`:
  - `sharedRoot` (default `~/.agentic`)
  - Per-tool: `enabled`, `skillsPath`, `agentsPath`, `rulesPath`, `instructionsPath`
- Four tool adapters: Codex, Claude Code, Cursor, OpenClaw
- Read-only scan of `<sharedRoot>/{skills,agents,rules}`
- Validation:
  - skill = directory containing `SKILL.md`
  - agent = `.md` file
  - rule = `.md` or `.mdc` file
- Per-tool state inspection: `enabled` / `disabled` / `broken` / `stale` / `foreign_file` / `foreign_link`
- Search by capability name or relative path
- Kind filter: all / skills / agents / rules
- Stage enable/disable changes before apply
- Safe apply semantics:
  - Create missing symlink
  - Remove correct symlink
  - Replace broken symlink
  - Replace wrong-target symlink
  - Create or refresh Cursor managed copies with stale detection
  - Block real-file and real-directory conflicts
  - Surface basename collisions in `flat` layout as `skip_conflict`
- Tool-specific projections:
  - Cursor agents: managed file copies under `~/.cursor/agents` with sync metadata sidecars
  - OpenClaw agents/skills: symlinks under `~/.openclaw/{agents,skills}`
  - Codex skills/agents: symlinks under `~/.agents/skills` and `~/.agents/agents` (matches OpenAI Codex's documented `$HOME/.agents/skills` user scope)
  - Claude skills/agents: flat symlinks at the top level of `~/.claude/skills/` and `~/.claude/agents/`
- Rule projection:
  - Cursor: `link_sync` via symlinks under `~/.cursor/rules`, preserving nested folders
  - Claude Code: `markdown_section_sync` into `~/.claude/CLAUDE.md`
  - Codex: `markdown_section_sync` into `~/.codex/AGENTS.md`
  - OpenClaw: `markdown_section_sync` into `~/.openclaw/workspace/SOUL.md`
- Apply result summary toast (created / removed / replaced / refreshed / skipped / errors)
- Manual refresh
- Progress events emitted from the Rust core for long applies, surfaced as inline progress in the UI

### Out of scope (this MVP)

- Suite presets (separate feature; see [suite-presets.md](./suite-presets.md))
- Workspace patch (separate feature; see [workspace-suite-sync.md](./workspace-suite-sync.md))
- Demo scaffold command (separate feature; see [agentic-demo-scaffold.md](./agentic-demo-scaffold.md))
- Background watchers
- Auto-detection of installed tools
- Import/export of manifests
- Workspace-local config (separate scope toggle)
- Multi-machine sync
- Rich preview of skill or agent content beyond filename + relative path
- Deep tree editing affordances

## Narrowest useful workflow

1. User launches Agentic Hub.
2. App reads settings, scans the shared root, inspects per-tool state.
3. User picks one tool tab (Codex / Claude Code / Cursor / OpenClaw).
4. App shows current state for that tool with `enabled` / `disabled` / `broken` / `stale` / `conflict` badges.
5. User searches and stages several changes.
6. User clicks Apply.
7. App applies safe filesystem changes and shows a result summary.

That is the MVP. Everything else is optional.

## Window layout

```
+--------------------------------------------------------------------+
| Header                                                              |
|   Agentic Hub      Shared root: ~/.agentic   [ Global | Workspace ] |
|   Tool tabs:  [Codex] [Claude] [Cursor] [OpenClaw]                  |
|   Search: [_______________]   Filter: [All|Skills|Agents|Rules]     |
|   Refresh   Settings                                                |
+--------------------------------------------------------------------+
| Body                                                                |
|   Left (capability list, grouped by kind):                          |
|     Skills                                                          |
|       [x] dev/repo-research               enabled                   |
|       [ ] arno/cto/code-review            disabled                  |
|       [!] marketing/repo-research         conflict (basename)       |
|     Agents                                                          |
|       [x] coding/coding-agent             enabled                   |
|     Rules                                                           |
|       [x] general/precise                 enabled                   |
|                                                                     |
|   Right (inspector + staged operations preview):                    |
|     Selected: dev/repo-research                                     |
|     Source: ~/.agentic/skills/dev/repo-research/SKILL.md            |
|     Codex target: ~/.agents/skills/dev/repo-research/               |
|     State: enabled                                                  |
|     Staged: 3 changes                                               |
|       + create ~/.agents/skills/arno/cto/code-review/               |
|       - remove ~/.agents/agents/legacy/foo.md                       |
|       ! skip   marketing/repo-research (basename collision)         |
+--------------------------------------------------------------------+
| Footer                                                              |
|   Staged: 3   [Apply Changes]   [Clear Staged]                      |
+--------------------------------------------------------------------+
```

## Default experience

- Open fast: cold start under 1.5s on macOS
- Show useful state immediately: scan + inspect runs on window mount, with skeleton placeholders during loading
- Keep browsing lightweight: search and filter are client-side over the in-memory scan result
- Make apply explicit: footer button, never an auto-apply on toggle

## Empty / error states

- **Invalid shared root** (missing or non-dir): empty state with a "Scaffold Demo Resources" button (delegates to [demo scaffold](./agentic-demo-scaffold.md)) and a "Choose Different Root" affordance
- **No capabilities found**: empty state suggesting that `<sharedRoot>/{skills,agents,rules}` directories should exist
- **Tool path missing**: tool tab disabled with a tooltip showing the missing path and a "Fix in Settings" link
- **Real-file conflict on apply**: result summary lists each conflict with its target path and the reason ("real file at target"); user must remove the file manually
- **Permission denied during apply**: result summary lists each failed op with reason; partial apply preserved

## State model

### Capability kinds

- `skill`
- `agent`
- `rule`

### Link states (per (tool, item))

- `enabled` — target is a symlink to the correct source (or managed copy with matching metadata)
- `disabled` — target does not exist
- `broken` — target is a symlink to a non-existent path
- `stale` — target is a managed copy whose metadata mismatches the shared source (Cursor agents only)
- `foreign_file` — target is a real file or directory not owned by the manager
- `foreign_link` — target is a symlink to a different source

`foreign_file` and `foreign_link` may be collapsed to a single `conflict` badge in the UI when that reduces complexity, but the host-side state is preserved distinctly so the apply path can react correctly.

### Staged operations

The UI maintains a per-tool `desiredEnabledByItemId` map. Toggling an item updates this map without touching disk. The footer's staged count is `count(desiredEnabledByItemId[id] != currentlyEnabled[id])`.

## Apply flow

1. UI gathers `desiredEnabledByItemId` for the focused tool.
2. UI invokes `cmd_plan({ toolId, desiredEnabledByItemId })`.
3. Rust core runs planner → returns `Vec<PlannedOperation>` (see [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md)).
4. UI shows confirmation if the plan contains any `skip_conflict` ops (so the user understands what will not happen).
5. UI invokes `cmd_apply({ operations })`.
6. Rust core executes operations one-by-one, emitting `apply-progress` events.
7. If the focused tool uses `markdown_section_sync`, UI invokes `cmd_sync_rules({ toolId, items, states })`.
8. UI invokes `cmd_inspect()` to refresh state.
9. UI shows `ApplyResult` summary toast.

## Acceptance criteria

- [ ] Opening the window with a valid shared root lists all available skills, agents, and rules
- [ ] Selecting any tool tab updates per-item state for that tool
- [ ] Search reduces the visible list without rescanning disk
- [ ] Kind filter reduces the visible list without rescanning disk
- [ ] Staging a toggle does not mutate disk
- [ ] Applying creates symlinks for newly-enabled items
- [ ] Applying removes symlinks for newly-disabled items
- [ ] Applying replaces broken symlinks with correct links
- [ ] Applying replaces wrong-target symlinks with correct links
- [ ] Applying never overwrites a real file or directory
- [ ] Enabling a Cursor agent writes a managed file copy (not a symlink) under `~/.cursor/agents` with a `<file>.e-studio-meta.json` sidecar
- [ ] Refreshing or re-enabling a stale Cursor agent rewrites the managed copy and updates the sidecar metadata
- [ ] Enabling a Cursor rule creates or repairs a symlink under `~/.cursor/rules` while preserving nested folders
- [ ] Applying Claude Code rule changes refreshes the managed marker block inside `~/.claude/CLAUDE.md`
- [ ] Applying Codex rule changes refreshes the managed marker block inside `~/.codex/AGENTS.md`
- [ ] Applying OpenClaw rule changes refreshes the managed marker block inside `~/.openclaw/workspace/SOUL.md`
- [ ] Two source skills with the same basename targeting Claude produce exactly one `create_link` and one `skip_conflict` op
- [ ] After apply, refresh reflects real disk state
- [ ] Invalid shared root and missing tool paths surface actionable errors
- [ ] App cold-start to first interactive window under 1.5s on macOS

## Dependencies

- Tauri 2.x shell + capability files (see [ARCHITECTURE.permissions.md](../../ARCHITECTURE.permissions.md))
- `agentic-core` modules: `settings`, `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync`
- IPC commands: `cmd_load_settings`, `cmd_save_settings`, `cmd_scan`, `cmd_inspect`, `cmd_plan`, `cmd_apply`, `cmd_sync_rules` (see [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md))

## Delivery slices

| Slice | What ships |
|-------|-----------|
| Phase 1: Read-only skeleton | Settings load/save, window shell, scan, render inventory + per-tool states |
| Phase 2: Staging + planning | Toggle staging, plan computation, plan preview, conflict surfacing |
| Phase 3: Safe mutations | Apply pipeline, rule sync, result summary |
| Phase 4: Hardening | Integration tests, error-path polish, docs |

## Risks and edge cases

- **Slow first scan on a large shared root.** Mitigation: cap walk depth, constrain to expected directories, surface progress.
- **Symlink cycles.** Mitigation: bounded walk depth in scanner.
- **External tool moves its config dir mid-session.** Mitigation: tool path checked at each plan/apply; tool tab disabled if path becomes unavailable.
- **Concurrent applies across windows.** Mitigation: serialize through a Tokio mutex in the Tauri shell (see [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md) Open Questions).
- **User edits a managed Cursor agent copy.** Mitigation: sync metadata's content hash detects drift; state becomes `stale`; user explicitly refreshes.

## Metrics or signals

- Time from cold launch to first interactive window
- Number of apply operations per session (signals adoption vs bash workflow)
- Apply error rate (target < 1%)
- Refresh frequency (signals trust in current state)

## Open questions

- Should we offer an inline plan preview before apply, or only show the result after? Current design: preview shown automatically when the plan contains any `skip_conflict` ops; otherwise direct apply.
- Should we show a cross-tool indicator in the inspector ("enabled for Codex, Cursor; disabled for Claude")? Defer to P2.
