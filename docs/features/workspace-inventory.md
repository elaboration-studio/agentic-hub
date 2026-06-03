# Feature: Workspace Inventory

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Depends On: [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md), [ARCHITECTURE.workspace.md](../../ARCHITECTURE.workspace.md)
Related Docs: [docs/tech/modules/workspace-inventory.md](../tech/modules/workspace-inventory.md), [docs/tech/modules/watcher.md](../tech/modules/watcher.md)

## Why now

The global manager answers "what does my machine have?" by reading the shared
roots and projecting them into each tool's home dir. But a developer opening a
specific project wants the inverse question: **"what agentic resources does
*this* project already give Cursor, Claude, and Codex?"** A project carries its
own `.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
`AGENTS.md`, `CLAUDE.md` — committed to the repo and shared with the team. There
was no way to audit that from the hub.

The earlier workspace flow hard-copied a suite *into* a project. That mixed the
hub's opinion into project files and only worked for resources that already
lived in the shared root. We removed it. Workspace scope is now a **read-only
audit**: pick a project, see exactly what each tool has, live-updated.

## What it does

- **Left rail of remembered workspaces.** Add a project via the folder dialog;
  it joins an LRU list (cap 12). Select one to make it active; remove when done.
- **Read-only inventory matrix.** The active workspace is scanned by walking each
  workspace tool's own directories. The result reuses the global manager's matrix
  render — rows are resources (skills, agents, rules, the tool instruction file),
  columns are tools, a cell is a static check when that tool has the resource.
- **Global × local in one view.** Because the same matrix component renders both
  scopes, a developer can flip the header toggle and compare what's installed
  globally on their machine against what a given project ships.
- **Live refresh.** Editing the project's tool dirs (dropping a skill, editing
  `AGENTS.md`) re-scans and updates the view via a `workspace-changed` event.
- **No writes, ever.** Workspace scope never touches the filesystem. Toggles,
  apply, and ownership locks are all inert in read-only mode.

## Scope

- Tools: Codex, Claude, Cursor (`WORKSPACE_TOOL_IDS`). OpenClaw is skipped until
  it has a stable project layout.
- Resource kinds: skills (`SKILL.md` dirs), agents (`*.md`), Cursor rules
  (`.cursor/rules/*.mdc|.md`), and each tool's instruction file as a single rule
  row.

## Out of scope (follow-ups)

- Parsing the Codex/Claude managed markdown block into individual rule rows.
- Inventorying installed hooks.
- Any write-back from the hub into a workspace.

## User flow

1. Toggle the header scope to **Workspace**.
2. Click **Add workspace…**, pick a project directory.
3. The project becomes active and its inventory renders in the matrix.
4. Edit the project's tool dirs in another tool — the matrix updates on save.
5. Remove the workspace from the rail when finished; nothing is left behind.
