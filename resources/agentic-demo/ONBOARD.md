# ONBOARD — agentic shared root

Fast path for a working layout next to **Agentic Hub**'s Capability Manager.

## Who this is for

- First-time setup after running **Agentic Hub: Scaffold Demo Resources**
- Adding skills discovered via the public `npx skills` CLI
- Keeping `.skill-lock.json`, `skills-lock.json`, and vendored `skills/...` in sync

## What lives here

- `agents/` — agent specs
- `skills/` — committed skill sources you own or vendor
- `rules/` — rules packs (for example Cursor)
- `hooks/` — per-event hook folders; each contains a `hook.json` manifest plus any sibling scripts the manifest's `command` references (resolve via `${HOOK_DIR}`)
- `.skill-lock.json` — **source of truth** for managed upstream skills (name, source, paths)
- `skills-lock.json` — generated lock consumed by `npx skills experimental_install`
- `.agents/skills/` — install output from `npx skills` (ignored by git via `.gitignore`)

## Prerequisites

- Node.js 20+ and npm (for `npx skills` and `npm run skills:*`)

## Managed skills workflow

This repo uses the same pattern as `~/.agentic-arno`: a small Node script wraps `npx skills` and syncs installs back into `skills/...`.

### Discover

```bash
npx skills find "<topic>"
```

Examples:

```bash
npx skills find react performance
npx skills find pr review
```

Browse the index at [skills.sh](https://skills.sh/).

### Add (try in a scratch folder if unsure)

```bash
npx skills add <owner/repo@skill>
```

### Record in `.skill-lock.json`

Add an entry under `skills` with `source`, `sourceType`, `sourceUrl`, and `skillPath` (see upstream docs for the exact shape you need). The bundled `scripts/skills-manage.mjs` expects this file to exist.

### Install / sync

From **this directory** (the shared root):

```bash
npm run skills:lock    # write skills-lock.json from .skill-lock.json
npm run skills:install # npx skills experimental_install + sync into skills/
npm run skills:check   # verify installs vs lock
npm run skills:update  # refresh upstream and sync
```

If `skills:check` reports missing folders, the upstream package may not expose a skill matching the lock entry name.

### Staying aligned with `npx skills`

- Prefer **lock → install → commit** over copying random folders into `skills/` by hand.
- For well-known upstream skills (for example `eze-is/web-access`), keep the GitHub source in `.skill-lock.json` and refresh with `npm run skills:update` rather than editing vendored files in isolation.

## Link into agent tools

Use **Agentic Hub: Open Capability Manager** to create symlinks safely, or mirror the manual pattern:

- Cursor: `~/.cursor/skills` and `~/.cursor/agents` (defaults in settings)
- Codex: `~/.agents/skills` and `~/.agents/agents` (matches OpenAI Codex's documented scan paths)
- Claude Code: `~/.claude/skills` and `~/.claude/agents`

Point each skill or agent entry in the shared root at the matching tool path via the panel so you do not overwrite real files by mistake.

## Hooks workflow

The scaffold ships one demo hook: `hooks/auto-format-after-edit/`. Its `hook.json` declares a `PostToolUse` event with `matcher: "Edit|Write"`, a `${HOOK_DIR}/script.sh` command, and `targets: ["cursor", "claude", "codex"]`.

To dogfood it end-to-end:

1. Open **Agentic Hub: Open Capability Manager** → pick **Cursor** (or Codex / Claude).
2. Expand the **Hooks** group, tick **Auto-format after edit**, click **Apply Changes**.
3. The extension projects a managed entry (with an inline `_agenticHub` marker) into the tool's hooks config:
   - Cursor: `~/.cursor/hooks.json` (event becomes `postToolUse`)
   - Claude: `~/.claude/settings.json` (event stays `PostToolUse`)
   - Codex: `~/.codex/hooks.json` (event stays `PostToolUse`)
4. Trigger an edit in that tool — `script.sh` runs and logs `[agentic-hub] auto-format-after-edit fired` to stderr.
5. Replace `script.sh` with your real formatter / linter / audit logic. The hash stored in the managed marker changes automatically on next apply.

Hook contract notes:

- `events[].name` uses canonical PascalCase (`PostToolUse`, `SessionStart`, `Stop`, `UserPromptSubmit`, …). The extension maps to each tool's native form (Cursor uses camelCase, Claude and Codex use PascalCase).
- `targets` controls which tools the hook is eligible for. If the focused tab in the Capability Manager isn't listed, the checkbox locks with a `Not Targeted` chip — click **Open** in the inspector to edit `hook.json` and broaden the array.
- `loopLimit` (optional, positive integer) projects as Cursor's `loop_limit` and is ignored by other tools — useful to prevent `Stop` hooks from re-entering themselves.
