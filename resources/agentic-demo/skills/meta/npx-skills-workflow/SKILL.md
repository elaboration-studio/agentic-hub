---
name: npx-skills-workflow
description: Discover, lock, install, and sync public skills with npx skills and .skill-lock.json — same workflow as ~/.agentic-arno, aligned with skills.sh and skills-lock.json.
---

# npx skills workflow

Use when the user wants to **find**, **install**, or **refresh** skills from the public registry (`npx skills`, [skills.sh](https://skills.sh/)), and keep a **durable lockfile** in sync with vendored `skills/...` folders.

## When to use

- "find a skill for X"
- "install a skill from GitHub"
- "keep my skills folder aligned with npx skills"
- "update locked upstream skills"

## Discover

```bash
npx skills find "<query>"
```

Try concrete queries (framework, task, tool name).

## Try an install

```bash
npx skills add <owner/repo@skill>
```

Use a throwaway directory first if you are unsure about the package.

## Lock in this shared root

1. Ensure `.skill-lock.json` exists (the demo scaffold ships an empty `skills` map).
2. Add an entry per upstream skill: `source`, `sourceType`, `sourceUrl`, `skillPath` (and optional `sourceSkillPath` when the skill is not at repo root).
3. Run from the shared root:

```bash
npm run skills:lock
npm run skills:install
npm run skills:check
```

## Refresh upstream

```bash
npm run skills:update
```

## Rules of thumb

- **Source of truth**: `.skill-lock.json` + git history under `skills/...`, not ad-hoc copies.
- **Generated**: `.agents/skills/` and `skills-lock.json` — reproduce with scripts, do not hand-edit `skills-lock.json` unless you know the `npx skills` format.
- **Safety**: `scripts/skills-manage.mjs` skips syncing over folders with local git changes unless you pass `--force`.

## Related doc

Read [`ONBOARD.md`](../../../ONBOARD.md) in this shared root for the full narrative and troubleshooting.
