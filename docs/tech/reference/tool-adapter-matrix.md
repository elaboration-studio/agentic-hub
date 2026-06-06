# Reference: Tool Adapter Matrix

Status: Stable
Mode: Detailed
Last Updated: 2026-06-04
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md), [docs/tech/modules/openclaw-tool-adapter.md](../modules/openclaw-tool-adapter.md), [docs/tech/modules/rule-projection-sync.md](../modules/rule-projection-sync.md), [docs/tech/modules/workspace-inventory.md](../modules/workspace-inventory.md)

## Purpose

Single-page reference table for every tool adapter: target paths, projection modes, layouts, and quirks. The source of truth for "where does X go when I enable it for Y?"

## Global scope (writes into tool home directories)

| Aspect | Codex | Claude Code | Cursor | OpenClaw | OpenStandard |
|--------|-------|-------------|--------|----------|--------------|
| Tool id | `codex` | `claude` | `cursor` | `openclaw` | `openstandard` |
| Default `enabled` | `true` | `true` | `true` | `true` | `true` |
| `skillsPath` default | `~/.codex/skills` | `~/.claude/skills` | `~/.cursor/skills` | `~/.openclaw/skills` | `~/.agents/skills` |
| `agentsPath` default | `~/.codex/agents` | `~/.claude/agents` | `~/.cursor/agents` | `~/.openclaw/agents` | `~/.agents/agents` |
| `rulesPath` default | `~/.codex/agentic-rules` | `~/.claude/rules` | `~/.cursor/rules` | `~/.openclaw/agentic-rules` | `~/.agents/rules` |
| `instructionsPath` default | `~/.codex/AGENTS.md` | `~/.claude/CLAUDE.md` | _unused_ | `~/.openclaw/workspace/SOUL.md` | `~/.agents/AGENTS.md` |
| `skillLayout` | `Nested` | `Flat` | `Nested` | `Nested` | `Nested` |
| `agentLayout` | `Nested` | `Nested` | `Nested` | `Nested` | `Nested` |
| Skill projection | symlink | **managed copy** (flat) | symlink | symlink | symlink |
| Agent projection | symlink | symlink (nested) | **managed copy** | symlink | symlink |
| Rule projection mode | `markdown_section_sync` | `markdown_section_sync` | `link_sync` | `markdown_section_sync` | `markdown_section_sync` |
| Rule target | `~/.codex/AGENTS.md` (managed block) | `~/.claude/CLAUDE.md` (managed block) | symlinks under `~/.cursor/rules/` | `~/.openclaw/workspace/SOUL.md` (managed block) | `~/.agents/AGENTS.md` (managed block) |
| Mirrored rule files | optional at `~/.codex/agentic-rules/` (annotated when present) | optional at `~/.claude/rules/` (rarely used) | n/a (rules are real files via symlink) | optional at `~/.openclaw/agentic-rules/` | optional at `~/.agents/rules/` |
| `hooks_enabled` default | `true` | `true` | `true` | `false` | `true` |
| `hooks_file` default | `~/.codex/hooks.json` | `~/.claude/settings.json` | `~/.cursor/hooks.json` | _none_ | `~/.agents/hooks.json` |
| Hook projection mode | `json_section` | `json_section` | `json_section` | not supported | `json_section` |
| Hook JSON shape | two-level (PascalCase events) | two-level (PascalCase events) | flat (camelCase events) | n/a | two-level (PascalCase events) |

### Codex is self-contained under `~/.codex`; OpenStandard owns `~/.agents`

The open-standard `~/.agents/` root (the convention OpenAI Codex documents for skills: `$HOME/.agents/skills`, walking up `$REPO_ROOT/.agents/skills`; see [Codex skills](https://developers.openai.com/codex/skills/)) is owned by its own first-class tool, **OpenStandard**, which projects skills, agents, rules, and hooks under `~/.agents/`. Codex's own tool entry is now fully self-contained under `~/.codex/` (skills, agents, rules, instructions, hooks), so the two columns are independent. Codex **subagents** remain TOML files under `~/.codex/agents/` (user) and `.codex/agents/` (project), each with `name` / `description` / `developer_instructions` ([Codex subagents](https://developers.openai.com/codex/subagents)). OpenStandard is **global-only** (like OpenClaw, but enabled by default).

### Why Cursor agents and Claude skills are managed copies

Cursor loads agent files into memory at launch, and Claude's skill loader does not follow symlinks — for both, a symlink is unreliable. Managed copies are real files/folders recorded in a per-root `.agentic-hub-managed.json` manifest (`{ version, entries: { <relPath>: { itemId, sourcePath, sourceHash } } }`) that lets us detect drift (`stale` state) and explicitly refresh on user action. For skill folders the `sourceHash` is the `SKILL.md` hash.

### Why only Claude *skills* are flat

Claude Code's **skill** loader scans only the top level of `~/.claude/skills/` — nested folders are not discovered ([docs](https://code.claude.com/docs/en/skills); issues [#18192](https://github.com/anthropics/claude-code/issues/18192) / [#10238](https://github.com/anthropics/claude-code/issues/10238)). Flat layout collapses a skill's `relative_path` to its basename. Claude **agents**, by contrast, are scanned **recursively** — `~/.claude/agents/` subfolders are honored and identity comes from the `name` frontmatter, not the path ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)) — so agents stay nested (flattening would collide same-basename agents). See [claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md).

## Workspace scope (writes into project directories)

OpenClaw is **not supported** in workspace scope.

Workspace scope is **read-only inventory** (`workspace_inventory`): it scans these per-tool dirs and reports what each tool already has. It never writes. The "scan source" column is the directory/file each tool actually reads.

| Aspect | Codex | Claude Code | Cursor |
|--------|-------|-------------|--------|
| Skills scan source | `<ws>/.agents/skills` | `<ws>/.claude/skills` | `<ws>/.cursor/skills` + `<ws>/.agents/skills` |
| Agents scan source | `<ws>/.codex/agents/*.toml` | `<ws>/.claude/agents/*.md` | `<ws>/.cursor/agents` + `<ws>/.agents/agents` |
| Rules scan source | (in `AGENTS.md`) | (in `CLAUDE.md`) | `<ws>/.cursor/rules/*.mdc` |
| Instructions scan source | `<ws>/AGENTS.md` | `<ws>/CLAUDE.md` | `<ws>/AGENTS.md` |
| Skill nesting | nested | nested | nested (recursive) |

Notes:
- **Codex agents are TOML** (`.codex/agents/*.toml`), distinct from Claude/Cursor markdown agents — the scanner matches `*.toml` for Codex, `*.md` otherwise.
- **`AGENTS.md` is read by both Codex and Cursor**, so the inventory attributes that row to both tools; `CLAUDE.md` is Claude-only.
- **Cursor also honors the shared `.agents/` dir** (skills) in addition to its own `.cursor/` dirs.
- Claude's flat skill constraint is a global-home loader quirk; the workspace scanner reports nested skills as-is because users own the project path.

## Per-kind projection summary

### Skill

- Global Codex / Cursor / OpenClaw / OpenStandard: symlink (nested)
- Global Claude: managed copy (flat) — Claude's skill loader does not follow symlinks
- Workspace: hard copy (nested)

### Agent

- Global Codex / Claude / OpenClaw / OpenStandard: symlink (nested for all — Claude scans `~/.claude/agents/` recursively)
- Global Cursor: managed copy (per-root manifest)
- Workspace: read-only inventory (Codex reads `.codex/agents/*.toml`; Claude/Cursor read `*.md`)

### Rule

- Global Cursor: symlink under `~/.cursor/rules/`
- Global Codex / Claude / OpenClaw / OpenStandard: managed block in instruction file
- Workspace Cursor: hard copy under `<ws>/.cursor/rules/`
- Workspace Codex / Claude: managed block in `<ws>/AGENTS.md` / `<ws>/CLAUDE.md`

### Hook

- Global Codex / Claude / Cursor / OpenStandard: managed JSON entry in the tool's hooks file (`json_section`)
- Global OpenClaw: not supported (no public hook spec)
- Workspace Codex / Claude / Cursor: managed JSON entry in `<ws>/.codex/hooks.json` / `<ws>/.claude/settings.json` / `<ws>/.cursor/hooks.json`
- Cursor uses a flat shape (camelCase events); Codex / Claude use a two-level shape (PascalCase events, marker on the matcher group). See [hook-projection-sync.md](../modules/hook-projection-sync.md).

## Operation kinds per projection mode

| Mode | Create | Update | Remove |
|------|--------|--------|--------|
| Symlink (`link_sync`) | `create_link` | `replace_link` | `remove_link` |
| Managed copy (`file_sync` with metadata) | `create_managed_copy` | `replace_managed_copy` | `remove_managed_copy` |
| Markdown section (`markdown_section_sync`) | full block rewrite via `rule_sync` | full block rewrite | block removal (file remains if other content present) |
| JSON section (`json_section`) | `sync_json_section` | `sync_json_section` | `clear_json_section` (foreign entries preserved) |

## State semantics per projection mode

| State | Symlink | Managed copy | Markdown section | JSON section |
|-------|---------|--------------|-------------------|--------------|
| `enabled` | Symlink points to correct source | Copy with manifest entry matching source + hash | Rule entry present in managed block | Managed entry present with matching `sourceHash` |
| `disabled` | Target absent | Target absent | Rule entry absent from managed block | No managed entry for this hook |
| `broken` | Symlink to non-existent path | n/a | n/a | Target JSON malformed/unreadable |
| `stale` | n/a | Manifest entry matches source path but content hash mismatched | n/a (rule sync rewrites on every apply) | Managed entry present but `sourceHash` mismatched |
| `foreign_file` | Real file at target | Real file/dir at target without a manifest entry | n/a | Target path is not a regular file |
| `foreign_link` | Symlink to different source | Manifest entry attributed to different source | n/a | n/a (foreign entries co-exist; never a conflict) |

## Decision logic: which projection mode does this `(tool, kind)` use?

```
fn projection_mode(tool: ToolId, kind: CapabilityKind, scope: SyncScope) -> ProjectionMode {
    match (tool, kind, scope) {
        // Hooks (OpenClaw has no hook support in any scope)
        (OpenClaw, Hook, _)          => Err(HookUnsupportedForTool),
        (_,        Hook, _)          => JsonSection,

        // Global scope
        (Cursor,   Agent, Global)    => ManagedCopy,
        (Claude,   Skill, Global)    => ManagedCopy,
        (Cursor,   Rule,  Global)    => LinkSync,
        (_,        Skill, Global)    => LinkSync,
        (_,        Agent, Global)    => LinkSync,
        (Cursor,   Rule,  Global)    => LinkSync,
        (_,        Rule,  Global)    => MarkdownSectionSync,

        // Workspace scope
        (Cursor,   Rule,  Workspace) => ManagedCopy,
        (Codex|Claude, Rule, Workspace) => MarkdownSectionSync,
        (_,        _,     Workspace) => ManagedCopy,
    }
}
```

OpenClaw + Workspace returns `Err(UnsupportedInWorkspaceScope)`. A hook targeting OpenClaw is dropped with a note (OpenClaw is never a default or supported hook target).

## Layout strategy per `(tool, kind)`

```
fn layout(tool: ToolId, kind: CapabilityKind, scope: SyncScope) -> Layout {
    match (tool, kind, scope) {
        (Claude, Skill, Global) => Flat,  // skill loader is non-recursive
        _ => Nested,                       // incl. Claude agents (recursive loader)
    }
}
```

Workspace scope always uses `Nested` (users own those paths fully).

## Why this lives in one table

If a developer asks "where does enabling skill X for tool Y go?", the answer should be one lookup, not a hunt through three files. The table is the contract; the module docs explain the why.

## Open questions

- Should we ever expose `skillLayout` and `agentLayout` as user-configurable settings? Decision: no in v1; layout is tool-intrinsic
- Should we eventually support a sixth tool (Aider, Continue, etc.)? Adding one is mostly: pick a tool id, add settings defaults, declare layout + projection mode, regression-test the planner. The `OpenStandard` addition (the open-standard `~/.agents` root as its own column) is the worked example.
