# Agentic resources (demo layout)

This folder is your **shared agentic root** for [Agentic Hub](https://github.com/SurfaceW/agentic-hub). The VS Code command **Agentic Hub: Scaffold Demo Resources** creates this layout under the path configured as `agentic-hub.sharedRoot` (default `~/.agentic`).

## Layout

| Path | Role |
| --- | --- |
| `skills/` | Reusable capability folders; each skill is a directory containing `SKILL.md`. |
| `agents/` | Agent specifications (markdown) you enable per tool. |
| `rules/` | Cross-cutting rules (for example Cursor `.mdc` files). |
| `hooks/` | Per-event hook folders; each hook is a directory containing `hook.json` (and any sibling scripts the manifest's `command` references via `${HOOK_DIR}`). |
| `scripts/skills-manage.mjs` | Lockfile bridge between `.skill-lock.json` and `npx skills` (`skills-lock.json`). |
| `.agents/skills/` | Generated installs when you use managed upstream skills (gitignored). |

## Quick start

1. In VS Code, run **Agentic Hub: Scaffold Demo Resources** (merge mode is safe for reruns).
2. Open **Agentic Hub: Open Capability Manager** and pick a tool (Cursor, Codex, Claude Code, OpenClaw).
3. Enable the demo skills and `agent-resources-manager` for that tool; apply the changes from the panel.
4. Read [`ONBOARD.md`](ONBOARD.md) for the `npx skills` discovery and install workflow so this tree stays aligned with the public skills ecosystem.

## Bundled demo content

- **Skill** `skills/agentic-hub/agentic-hub-setup` — build and validate the `agentic-hub` repo from `AGENTS.md`.
- **Skill** `skills/meta/npx-skills-workflow` — how to use `npx skills find` / `add` with `.skill-lock.json` and `npm run skills:*`.
- **Agent** `agents/agent-resources-manager.md` — how to reason about this folder and the capability manager.
- **Hook** `hooks/auto-format-after-edit/` — runs `script.sh` on `PostToolUse` for `Edit|Write`, targets all three tools (`cursor`, `claude`, `codex`). Enable it from the Capability Manager's **Hooks** group; replace `script.sh` with your real formatter / linter / audit logic. The manifest's `command` uses `${HOOK_DIR}` so sibling scripts resolve to absolute paths at projection time.

For a fuller personal library (many more skills and agents), clone or copy patterns from a maintained tree such as `~/.agentic-arno` and point the extension shared root at this folder or that clone.
