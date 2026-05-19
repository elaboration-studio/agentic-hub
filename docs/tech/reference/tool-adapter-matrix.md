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
| Skill projection | symlink | symlink (flat) | symlink | symlink |
| Agent projection | symlink | symlink (flat) | **managed copy** | symlink |
| Rule projection mode | `markdown_section_sync` | `markdown_section_sync` | `link_sync` | `markdown_section_sync` |
| Rule target | `~/.codex/AGENTS.md` (managed block) | `~/.claude/CLAUDE.md` (managed block) | symlinks under `~/.cursor/rules/` | `~/.openclaw/workspace/SOUL.md` (managed block) |
| Mirrored rule files | optional at `~/.codex/agentic-rules/` (annotated when present) | optional at `~/.claude/rules/` (rarely used) | n/a (rules are real files via symlink) | optional at `~/.openclaw/agentic-rules/` |

### Why Codex uses `~/.agents/`

OpenAI Codex's documented skill scan paths are `$CWD/.agents/skills` walking up to `$REPO_ROOT/.agents/skills`, and `$HOME/.agents/skills` for user scope. Agents follow the same `.agents/` root for consistency. This is distinct from Codex's tool config dir `~/.codex/` which is used for `AGENTS.md` instruction projection.

### Why Cursor agents are managed copies

Cursor loads agent files into memory at launch. Symlinks may be followed once and not re-checked. Managed copies with metadata sidecars (`<file>.e-studio-meta.json`) let us detect drift (`stale` state) and explicitly refresh on user action.

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

- Global: symlink (flat for Claude, nested otherwise)
- Workspace: hard copy (nested)

### Agent

- Global Codex / Claude / OpenClaw: symlink (flat for Claude)
- Global Cursor: managed copy with metadata sidecar
- Workspace: hard copy (nested)

### Rule

- Global Cursor: symlink under `~/.cursor/rules/`
- Global Codex / Claude / OpenClaw: managed block in instruction file
- Workspace Cursor: hard copy under `<ws>/.cursor/rules/`
- Workspace Codex / Claude: managed block in `<ws>/AGENTS.md` / `<ws>/CLAUDE.md`

## Operation kinds per projection mode

| Mode | Create | Update | Remove |
|------|--------|--------|--------|
| Symlink (`link_sync`) | `create_link` | `replace_link` | `remove_link` |
| Managed copy (`file_sync` with metadata) | `create_managed_copy` | `replace_managed_copy` | `remove_managed_copy` |
| Markdown section (`markdown_section_sync`) | full block rewrite via `rule_sync` | full block rewrite | block removal (file remains if other content present) |

## State semantics per projection mode

| State | Symlink | Managed copy | Markdown section |
|-------|---------|--------------|-------------------|
| `enabled` | Symlink points to correct source | Copy with metadata sidecar matching source + hash | Rule entry present in managed block |
| `disabled` | Target absent | Target absent | Rule entry absent from managed block |
| `broken` | Symlink to non-existent path | n/a | n/a |
| `stale` | n/a | Sidecar matches source path but content hash mismatched | n/a (rule sync rewrites on every apply) |
| `foreign_file` | Real file at target | Real file at target without sidecar | n/a |
| `foreign_link` | Symlink to different source | Managed copy attributed to different source | n/a |

## Decision logic: which projection mode does this `(tool, kind)` use?

```
fn projection_mode(tool: ToolId, kind: CapabilityKind, scope: SyncScope) -> ProjectionMode {
    match (tool, kind, scope) {
        // Global scope
        (Cursor,   Agent, Global)    => ManagedCopy,
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

OpenClaw + Workspace returns `Err(UnsupportedInWorkspaceScope)`.

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
