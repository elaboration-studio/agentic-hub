# Reference: Tool Adapter Matrix

Status: Stable
Mode: Detailed
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md), [docs/tech/modules/openclaw-tool-adapter.md](../modules/openclaw-tool-adapter.md), [docs/tech/modules/rule-projection-sync.md](../modules/rule-projection-sync.md), [docs/tech/modules/workspace-patch.md](../modules/workspace-patch.md)

## Purpose

Single-page reference table for every tool adapter: target paths, projection modes, layouts, and quirks. The source of truth for "where does X go when I enable it for Y?"

## Global scope (writes into tool home directories)

| Aspect | Codex | Claude Code | Cursor | OpenClaw |
|--------|-------|-------------|--------|----------|
| Tool id | `codex` | `claude` | `cursor` | `openclaw` |
| Default `enabled` | `true` | `true` | `true` | `true` |
| `skillsPath` default | `~/.agents/skills` | `~/.claude/skills` | `~/.cursor/skills` | `~/.openclaw/skills` |
| `agentsPath` default | `~/.agents/agents` | `~/.claude/agents` | `~/.cursor/agents` | `~/.openclaw/agents` |
| `rulesPath` default | `~/.codex/agentic-rules` | `~/.claude/rules` | `~/.cursor/rules` | `~/.openclaw/agentic-rules` |
| `instructionsPath` default | `~/.codex/AGENTS.md` | `~/.claude/CLAUDE.md` | _unused_ | `~/.openclaw/workspace/SOUL.md` |
| `skillLayout` | `Nested` | `Flat` | `Nested` | `Nested` |
| `agentLayout` | `Nested` | `Flat` | `Nested` | `Nested` |
| Skill projection | symlink | **managed copy** (flat) | symlink | symlink |
| Agent projection | symlink | symlink (flat) | **managed copy** | symlink |
| Rule projection mode | `markdown_section_sync` | `markdown_section_sync` | `link_sync` | `markdown_section_sync` |
| Rule target | `~/.codex/AGENTS.md` (managed block) | `~/.claude/CLAUDE.md` (managed block) | symlinks under `~/.cursor/rules/` | `~/.openclaw/workspace/SOUL.md` (managed block) |
| Mirrored rule files | optional at `~/.codex/agentic-rules/` (annotated when present) | optional at `~/.claude/rules/` (rarely used) | n/a (rules are real files via symlink) | optional at `~/.openclaw/agentic-rules/` |
| `hooks_enabled` default | `true` | `true` | `true` | `false` |
| `hooks_file` default | `~/.codex/hooks.json` | `~/.claude/settings.json` | `~/.cursor/hooks.json` | _none_ |
| Hook projection mode | `json_section` | `json_section` | `json_section` | not supported |
| Hook JSON shape | two-level (PascalCase events) | two-level (PascalCase events) | flat (camelCase events) | n/a |

### Why Codex uses `~/.agents/`

OpenAI Codex's documented skill scan paths are `$CWD/.agents/skills` walking up to `$REPO_ROOT/.agents/skills`, and `$HOME/.agents/skills` for user scope. Agents follow the same `.agents/` root for consistency. This is distinct from Codex's tool config dir `~/.codex/` which is used for `AGENTS.md` instruction projection.

### Why Cursor agents and Claude skills are managed copies

Cursor loads agent files into memory at launch, and Claude's skill loader does not follow symlinks — for both, a symlink is unreliable. Managed copies are real files/folders recorded in a per-root `.agentic-hub-managed.json` manifest (`{ version, entries: { <relPath>: { itemId, sourcePath, sourceHash } } }`) that lets us detect drift (`stale` state) and explicitly refresh on user action. For skill folders the `sourceHash` is the `SKILL.md` hash.

### Why Claude is flat

Claude Code's loader scans only the top level of `~/.claude/skills/` and `~/.claude/agents/`. Nested folders are treated as opaque single entries. Flat layout collapses the source's `relative_path` to its basename for the target path. See [claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md).

## Workspace scope (writes into project directories)

OpenClaw is **not supported** in workspace scope.

| Aspect | Codex | Claude Code | Cursor |
|--------|-------|-------------|--------|
| `skills_path` | `<ws>/.agents/skills` | `<ws>/.claude/skills` | `<ws>/.cursor/skills` |
| `agents_path` | `<ws>/.agents/agents` | `<ws>/.claude/agents` | `<ws>/.cursor/agents` |
| `rules_path` | `<ws>/.codex/agentic-rules` | `<ws>/.claude/agentic-rules` | `<ws>/.cursor/rules` |
| `instructions_path` | `<ws>/AGENTS.md` | `<ws>/CLAUDE.md` | _unused_ |
| Skill projection | hard copy (deref symlinks) | hard copy | hard copy |
| Agent projection | hard copy | hard copy | hard copy |
| Rule projection mode | `markdown_section_sync` | `markdown_section_sync` | `file_sync` (raw copy) |
| Layout | `Nested` | `Nested` | `Nested` |

In workspace scope every projection is hard copy (no symlinks). Claude's flat constraint only applies to its global home; workspace `<ws>/.claude/skills/` can be nested because users own that path entirely.

## Per-kind projection summary

### Skill

- Global Codex / Cursor / OpenClaw: symlink (nested)
- Global Claude: managed copy (flat) — Claude's skill loader does not follow symlinks
- Workspace: hard copy (nested)

### Agent

- Global Codex / Claude / OpenClaw: symlink (flat for Claude)
- Global Cursor: managed copy (per-root manifest)
- Workspace: hard copy (nested)

### Rule

- Global Cursor: symlink under `~/.cursor/rules/`
- Global Codex / Claude / OpenClaw: managed block in instruction file
- Workspace Cursor: hard copy under `<ws>/.cursor/rules/`
- Workspace Codex / Claude: managed block in `<ws>/AGENTS.md` / `<ws>/CLAUDE.md`

### Hook

- Global Codex / Claude / Cursor: managed JSON entry in the tool's hooks file (`json_section`)
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
        (Claude, Skill, Global) => Flat,
        (Claude, Agent, Global) => Flat,
        _ => Nested,
    }
}
```

Workspace scope always uses `Nested` (users own those paths fully).

## Why this lives in one table

If a developer asks "where does enabling skill X for tool Y go?", the answer should be one lookup, not a hunt through three files. The table is the contract; the module docs explain the why.

## Open questions

- Should we ever expose `skillLayout` and `agentLayout` as user-configurable settings? Decision: no in v1; layout is tool-intrinsic
- Should we eventually support a fifth tool (Aider, Continue, etc.)? Adding one is mostly: pick a tool id, add settings defaults, declare layout + projection mode, regression-test the planner. Defer until a real demand surfaces.
