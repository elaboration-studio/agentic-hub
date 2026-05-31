# Reference: Shared Root Contract

Status: Stable
Mode: Detailed
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/agentic-demo-scaffold.md](../../features/agentic-demo-scaffold.md), [docs/tech/modules/rule-projection-sync.md](../modules/rule-projection-sync.md)

## Purpose

Define the filesystem contract for the **shared agentic root** that Agentic Hub scans, validates, and projects into tool homes.

## Default location

`~/.agentic`

User-configurable via `settings.sources` (`~/.agentic-hub/config.json`) — an ordered list of `{ label, path }` source folders, each of which is a shared root that follows the contract on this page. When `sources` is empty, the legacy `settings.shared_root` is treated as a single `Default` source for one release. Items are keyed across the forest by `${kind}:${relative_path}`; the first (highest-priority) source wins and a later duplicate is dropped and reported as a scan error. See [multi-source-roots.md](../modules/multi-source-roots.md).

## Top-level shape

```
<sharedRoot>/
  skills/
  agents/
  rules/
  hooks/
```

Only these four top-level directories are walked by the scanner. Other top-level entries (READMEs, scripts, lock files) are ignored. Any directory named `__archived__` is skipped at every depth (see [What is intentionally ignored](#what-is-intentionally-ignored)).

A populated example:

```
~/.agentic/
  README.md                    # optional, ignored by scanner
  ONBOARD.md                   # optional, ignored
  skills/
    agentic-hub-setup/
      SKILL.md
    dev/
      repo-research/
        SKILL.md
        prompts.md
    arno/
      cto/
        code-review/
          SKILL.md
  agents/
    coding/
      coding-agent.md
    research-agent.md
  rules/
    general/
      precise.mdc
      workspace.mdc
    frontend/
      components.md
```

## Capability validation

### Skills

A **skill** is a directory containing `SKILL.md`.

| Path | Valid? | Reason |
|------|--------|--------|
| `~/.agentic/skills/foo/SKILL.md` | yes | Directory `foo/` contains `SKILL.md` |
| `~/.agentic/skills/foo/` (no SKILL.md inside) | no | Missing `SKILL.md` |
| `~/.agentic/skills/dev/repo-research/SKILL.md` | yes | Nested category is fine |
| `~/.agentic/skills/foo.md` (file, not dir) | no | Skills must be directories |

The `relative_path` is the directory path under `<sharedRoot>/skills/`. The `source_path` is the absolute directory path. The scanner reads `SKILL.md` for validation only — its content is not parsed by the scanner.

### Agents

An **agent** is a `.md` file under `<sharedRoot>/agents/`.

| Path | Valid? |
|------|--------|
| `~/.agentic/agents/foo.md` | yes |
| `~/.agentic/agents/group/bar.md` | yes |
| `~/.agentic/agents/foo/` (directory) | no |
| `~/.agentic/agents/foo.txt` | no |

The `relative_path` is the file path under `<sharedRoot>/agents/`, including extension. The `source_path` is the absolute file path.

### Rules

A **rule** is a `.md` or `.mdc` file under `<sharedRoot>/rules/`.

| Path | Valid? |
|------|--------|
| `~/.agentic/rules/general/precise.mdc` | yes |
| `~/.agentic/rules/components.md` | yes |
| `~/.agentic/rules/general/notes.txt` | no |

The `relative_path` is the file path under `<sharedRoot>/rules/`, including extension.

### Hooks

A **hook** is a directory under `<sharedRoot>/hooks/` containing a `hook.json` manifest, optionally with sibling scripts (e.g. `script.sh`).

| Path | Valid? | Reason |
|------|--------|--------|
| `~/.agentic/hooks/auto-format-after-edit/hook.json` | yes | Directory contains `hook.json` |
| `~/.agentic/hooks/auto-format-after-edit/` (no `hook.json`) | no | Missing manifest |
| `~/.agentic/hooks/foo.json` (file, not dir) | no | Hooks must be directories |

The `relative_path` is the directory path under `<sharedRoot>/hooks/`. The `source_path` is the absolute directory path — this is what `${HOOK_DIR}` resolves to at projection time. The scanner parses `hook.json` to validate it (see [hook-projection-sync.md](../modules/hook-projection-sync.md) for the schema and validation rules).

## Capability IDs

Stable identifiers used everywhere (state inspection, suites, manifests):

| Kind | ID format | Example |
|------|-----------|---------|
| Skill | `skill:<relative_path_without_skill_md>` | `skill:dev/repo-research` |
| Agent | `agent:<relative_path>` | `agent:coding/coding-agent.md` |
| Rule | `rule:<relative_path>` | `rule:general/precise.mdc` |
| Hook | `hook:<relative_path>` | `hook:auto-format-after-edit` |

IDs are stable as long as the source location does not change. Renaming a skill changes its ID; suites referencing the old ID treat it as stale.

## Symlinks inside the shared root

The scanner follows symlinks that resolve inside the shared root or to any path the user has access to. Common use: bundling skills from multiple sources, e.g.

```
~/.agentic/skills/imported/foo  -> ~/.agentic-arno/skills/foo
```

The validation rule (`SKILL.md` exists) is checked against the resolved target.

Cycles are detected with a bounded walk depth (16 levels by default).

## YAML frontmatter

Rules may have YAML frontmatter; the rule sync strips it before inlining into the managed block:

```mdc
---
description: Be precise and clear
applies-to: all
---
> Be precise.
```

Skills' `SKILL.md` files also commonly have YAML frontmatter (per the conventional skill format), but the scanner does not parse it — only verifies the file exists.

## What is intentionally ignored

- Top-level files (`README.md`, `ONBOARD.md`, etc.) — informational only
- Directories outside `skills/`, `agents/`, `rules/`, `hooks/` — e.g. `scripts/`, `node_modules/`
- Any directory named `__archived__`, at any depth — reserved for old versions of files per the workspace convention; never scanned
- Hidden files (anything starting with `.`) — convention only; `.skill-lock.json` is ignored not because of the dot but because it lives in the top level
- Files that don't match the allowed extensions per kind

This means the user can add tooling, READMEs, and lock files to the shared root without confusing the scanner.

## Recommended shared-root contents

The bundled demo scaffold provides a working starting point. Beyond that, users typically add:

- A `README.md` explaining the user's personal conventions
- An `ONBOARD.md` explaining how the shared root is used across tools
- A `package.json` + `scripts/skills-manage.mjs` for the optional `npx skills` lock workflow
- A `.skill-lock.json` or `skills-lock.json` for skill-version pinning (consumed by the optional lock workflow, not by Agentic Hub)

None of these are required. Agentic Hub works with a shared root containing only `skills/`, `agents/`, and `rules/`.

## Path validation rules

- All paths are tilde-expanded and canonicalized before use
- The shared root must be a directory; if it is a regular file, the scanner returns a clear error
- The shared root must be under the user's home directory by default (sanity check; configurable for power users)

## Open questions

- Should we add a top-level `prompts/` directory as a fifth kind? Defer; users who want prompt libraries can use skills or rules
- ~~Should we support multiple shared roots (overlayed)?~~ Resolved: shipped as ordered `sources` with first-source-wins dedupe — see [multi-source-roots.md](../modules/multi-source-roots.md)
