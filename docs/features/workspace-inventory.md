# Feature: Workspace Inventory

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Depends On: [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md), [ARCHITECTURE.workspace.md](../../ARCHITECTURE.workspace.md)
Related Docs: [docs/tech/modules/workspace-inventory.md](../tech/modules/workspace-inventory.md), [docs/tech/modules/watcher.md](../tech/modules/watcher.md), [docs/features/command-palette.md](./command-palette.md)

## Why now

The global manager answers "what does my machine have?" by reading the shared
roots and projecting them into each tool's home dir. But a developer opening a
specific project wants the inverse question: **"what agentic resources does
*this* project already give each supported agentic tool?"** A project carries its
own `.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
`AGENTS.md`, `CLAUDE.md` — committed to the repo and shared with the team. There
was no way to audit that from the hub.

The earlier workspace flow hard-copied a suite *into* a project. That mixed the
hub's opinion into project files and only worked for resources that already
lived in the shared root. We removed it. Workspace scope is now a **read-only
audit**: pick a project, see exactly what each tool has, live-updated.

## What it does

- **One unified rail (Global pinned + workspaces).** A single left rail drives
  the whole Manager: **Global** sits pinned at the top (the "what does my machine
  have?" view), with remembered workspaces listed below it. Add a project via the
  folder dialog; it joins an LRU list (cap 12). Click an entry to switch scope —
  Global or that workspace — and remove a workspace when done. This replaces the
  old header scope `Select`; there is no separate switcher.
- **Filters reset on scope switch.** Switching between Global and Workspace clears
  the scope-specific filters (source, enabled-only, collapsed groups, locate
  highlight) because a source id valid in one scope often doesn't exist in the
  other; the universal `query` / `kind` / `view` are kept.
- **Read-only inventory matrix.** The active workspace is scanned by walking each
  workspace tool's own directories. The result reuses the global manager's matrix
  render — rows are resources (skills, agents, rules, the tool instruction file),
  columns are tools, a cell is a static check when that tool has the resource.
- **Global × local in one view.** The workspace matrix merges the project's local
  resources with the globally-applied ones (resources projected into each tool's
  home dir apply to every project). Each row is tagged with its **source** — a
  shared root (`.agentic-arno`, `.helper`, …) or `Workspace` — and a **source
  filter** lets the developer narrow to just the local resources or to any one
  shared directory. This shows, at a glance, every resource that actually applies
  to the project, global and local together.
- **Cursor reads `.agents/` too.** Cursor honors the shared `.agents/` standard
  dir as well as its own `.cursor/`, so skills/agents dropped in `.agents/` show
  up under both Codex and Cursor.
- **Live refresh.** Editing the project's tool dirs (dropping a skill, editing
  `AGENTS.md`) re-scans via `workspace-changed`; a change to a shared root
  (`sources-changed`) reloads the merged inventory too.
- **No writes, ever.** Workspace scope never touches the filesystem. Toggles,
  apply, and ownership locks are all inert in read-only mode.
- **Same read-only primitive as Global installed resources.** The workspace
  scanner and the Global installed-resource scanner share the same inventory
  contract: discovered tool-native resources become source-labeled rows with
  static present cells. The Global feature uses tool labels (`Codex`, `Kiro`,
  `Copilot`, …); Workspace scope keeps the single `Workspace` label.
- **Locate from the command palette.** The
  [command palette](./command-palette.md) searches every remembered workspace's
  inventory. Picking a result *locates* it: the Hub jumps to Workspace scope,
  activates the owning workspace, and scrolls/highlights that row in the matrix —
  a fast path into this view from anywhere.

## Scope

- Tools: Codex, Claude, Cursor, Kiro, Copilot, and Antigravity
  (`WORKSPACE_TOOL_IDS`). OpenClaw and OpenStandard are global-only.
- Resource kinds: tool-specific skills, agents, rule files, and instruction
  files. Kiro and Copilot per-hook JSON files are also inventoried.

## Out of scope (follow-ups)

- Parsing the Codex/Claude managed markdown block into individual rule rows.
- Decomposing aggregate hook files for Codex, Claude, Cursor, or Antigravity.
- Any write-back from the hub into a workspace.

## User flow

1. In the left rail, click **Add workspace…**, pick a project directory.
2. The project becomes active (scope switches from Global to that workspace); the
   matrix renders its local resources merged with the globally-applied ones, each
   tagged by source. Click **Global** at the top of the rail anytime to switch back.
3. Use the **source filter** to narrow to `Workspace` (local only) or to a single
   shared root.
4. Edit the project's tool dirs in another tool — the matrix updates on save.
5. Remove the workspace from the rail when finished; nothing is left behind.

Alternatively, summon the command palette from anywhere, type a resource name,
and pick a workspace hit — the Hub lands on this view with that row highlighted.
