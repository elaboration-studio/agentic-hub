# Module: OpenClaw Tool Adapter

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../../features/mvp-unified-agentic-capability-manager.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Purpose

Document how Agentic Hub integrates with OpenClaw — an open-source AI coding assistant with a gateway, multi-agent routing, and channel support. Covers filesystem conventions, projection strategy, managed-block contract, and settings defaults.

## OpenClaw filesystem conventions

OpenClaw stores everything under `~/.openclaw/`:

```
~/.openclaw/
  openclaw.json          # config (JSON5)
  workspace/
    SOUL.md              # agent constitution — loaded first at every session start
  skills/                # managed local skills
  agents/                # per-agent directories
    <agent-id>/
      SOUL.md            # per-agent personality override
  agentic-rules/         # optional mirror dir for shared agentic rules (not natively read)
```

`SOUL.md` is the primary instruction file. It defines personality, values, communication style, and behavioral boundaries. OpenClaw loads it before `IDENTITY.md`, `USER.md`, `MEMORY.md`, and `AGENTS.md` on session start, which makes it the correct target for `markdown_section_sync`.

## Projection strategy

| Capability kind | Projection | Target |
|-----------------|------------|--------|
| `rule` | `markdown_section_sync` | `~/.openclaw/workspace/SOUL.md` |
| `agent` | `link_sync` (symlink) | `~/.openclaw/agents/` |
| `skill` | `link_sync` (symlink) | `~/.openclaw/skills/` |

OpenClaw agents do **not** use managed copies (unlike Cursor). Symlinks into `~/.openclaw/agents/` are sufficient because OpenClaw resolves agent files at runtime rather than caching them at launch.

Skill and agent layout: `Nested` (OpenClaw recurses by default).

## Managed block in SOUL.md

Agentic Hub owns exactly one block in SOUL.md delimited by the standard markers:

```md
<!-- agentic-hub:start -->
## Agentic Hub Managed Rules

This section is managed by Agentic Hub. Edit rule selections in the Capability Manager instead of editing these blocks by hand.

### general/precise.mdc

Source: `~/.agentic/rules/general/precise.mdc`
Mirrored link: `~/.openclaw/agentic-rules/general/precise.mdc`

...rule body with frontmatter stripped...
<!-- agentic-hub:end -->
```

Rules:
- The managed block is appended after existing SOUL.md content on first sync
- All content outside the markers is preserved unchanged
- If no rules are enabled, the managed block is removed; the file is deleted only when the managed block was its entire content
- Malformed markers (start without end, or end before start) are reported as `RuleSyncError::MalformedMarkers` and block rewrite until a fresh apply can replace the section safely

## Settings defaults

Persisted under `tools.openclaw` in `~/.agentic-hub/config.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `true` | Show the OpenClaw tab and include it in apply operations |
| `skillsPath` | `~/.openclaw/skills` | Target directory for symlinked skills |
| `agentsPath` | `~/.openclaw/agents` | Target directory for symlinked agents |
| `rulesPath` | `~/.openclaw/agentic-rules` | Optional mirror directory; not natively read by OpenClaw, used only for `Mirrored link:` annotation in SOUL.md |
| `instructionsPath` | `~/.openclaw/workspace/SOUL.md` | Path to SOUL.md (managed rules block target) |

## Skills path precedence in OpenClaw

OpenClaw loads skills from multiple locations with the following precedence (highest first):

1. `/skills` in the current workspace
2. `/.agents/skills` in the project
3. `~/.agents/skills` (personal agent skills)
4. `~/.openclaw/skills` (managed local skills — Agentic Hub's target)
5. Bundled skills
6. `skills.load.extraDirs` in `openclaw.json`

Agentic Hub symlinks shared skills into `~/.openclaw/skills`, placing them at priority 4. This is intentionally below workspace and project-level skills so local overrides remain in control.

## What the adapter registry does for OpenClaw

No OpenClaw-specific branches are added to `adapter_registry`. OpenClaw is handled via the existing generic logic:

- `projection_kind_for(kind)`:
  - `Rule` → `tool.rule_projection` (`MarkdownSectionSync`)
  - `Agent` / `Skill` → `LinkSync` (the Cursor managed-copy guard does not fire because tool id is not Cursor)
- `target_path_for(item)`:
  - Rules use the `instructions_path` (sync target is the SOUL.md file, not a per-rule path)
  - Skills and agents use `item.relative_path` unchanged (Claude flat-layout guard does not fire because tool id is not Claude)

## Workspace scope

OpenClaw is **not supported in workspace scope** in v1. `adapter_registry::create_workspace_adapter(OpenClaw, ...)` returns `Err(UnsupportedInWorkspaceScope)`. This matches the original VS Code feature scope decision.

Reasoning: OpenClaw does not have a stable documented project-level scan path equivalent to Codex's `<project>/.agents/skills`. Revisit if/when that lands.

## Conflict handling

Mirrors the general policy in [rule-projection-sync.md](./rule-projection-sync.md):

- If `~/.openclaw/workspace/SOUL.md` is a directory or non-file target, the manager reports a conflict and does not write
- If markers are malformed, the manager reports `MalformedMarkers` and requires a manual fix before rewrite
- If `~/.openclaw/workspace/` does not exist, the rule sync creates the directory hierarchy via `mkdir -p` before writing SOUL.md

## Tests

- Unit: SOUL.md managed-block writer (shared with other `markdown_section_sync` tools)
- Integration:
  - Enable a rule for OpenClaw → SOUL.md gets managed block appended at end (if no markers existed)
  - Disable all rules for OpenClaw → managed block removed, rest of SOUL.md preserved
  - Enable a skill for OpenClaw → symlink at `~/.openclaw/skills/<rel-path>`
  - Workspace apply with OpenClaw selected → refused with clear error

## Out of scope

- Per-agent SOUL.md overrides (`~/.openclaw/agents/<id>/SOUL.md`) — only the default workspace SOUL.md is managed
- `openclaw.json` mutations — the manager does not write to the OpenClaw config file
- OpenClaw channel or gateway configuration
- Workspace scope (deferred; not supported in v1)

## Open questions

- Should we add per-agent SOUL.md overrides as a P2 feature? Defer; existing single-SOUL projection covers the primary use case
- Should we read `openclaw.json` to detect non-default `skills.load.extraDirs` and warn about precedence shadowing? Defer to P2
