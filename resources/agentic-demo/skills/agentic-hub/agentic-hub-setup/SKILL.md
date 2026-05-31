---
name: agentic-hub-setup
description: Build, lint, test, and package the Agentic Hub VS Code extension using its pnpm workspace.
---

# agentic-hub setup

Use this skill when working in the **agentic-hub** repository (VS Code extension).

## Scope

- Primary product: `packages/vs-code/` (Agentic Hub extension).
- Contract: `AGENTS.md` at repo root.

## Commands (from repo root)

| Command | Purpose |
| --- | --- |
| `pnpm --filter agentic-hub compile` | Dev build of the extension bundle |
| `pnpm --filter agentic-hub watch` | Rebuild on change |
| `pnpm --filter agentic-hub lint` | ESLint on `packages/vs-code/src` |
| `pnpm --filter agentic-hub test` | VS Code extension tests |
| `pnpm --filter agentic-hub vsce:package` | Produce `.vsix` |

## Workflow

1. Read `AGENTS.md` for directory map, commands, and conventions.
2. Keep manifest (`packages/vs-code/package.json`) aligned with new commands or settings.
3. Run `lint` and `compile` (or `test`) before declaring the slice done.

## Agentic resources

- Shared root defaults to `~/.agentic` (VS Code setting `agentic-hub.sharedRoot`).
- Use **Agentic Hub: Open Capability Manager** to enable this skill for Cursor, Codex, Claude Code, or OpenClaw.
