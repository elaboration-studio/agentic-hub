# Reference: Tool Adapter Matrix

Status: Stable
Mode: Detailed
Last Updated: 2026-06-30
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md), [docs/tech/modules/kiro-tool-adapter.md](../modules/kiro-tool-adapter.md), [docs/tech/modules/openclaw-tool-adapter.md](../modules/openclaw-tool-adapter.md), [docs/tech/modules/rule-projection-sync.md](../modules/rule-projection-sync.md), [docs/tech/modules/workspace-inventory.md](../modules/workspace-inventory.md)

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
| `commandsPath` default | `~/.codex/prompts` | `~/.claude/commands` | `~/.cursor/commands` | _none_ | `~/.agents/commands` |
| `instructionsPath` default | `~/.codex/AGENTS.md` | `~/.claude/CLAUDE.md` | _unused_ | `~/.openclaw/workspace/SOUL.md` | `~/.agents/AGENTS.md` |
| `skillLayout` | `Nested` | `Flat` | `Nested` | `Nested` | `Nested` |
| `agentLayout` | `Flat` | `Nested` | `Flat` | `Nested` | `Nested` |
| Skill projection | symlink | **managed copy** (flat) | symlink | symlink | symlink |
| Agent projection | **managed copy → TOML** (`codex_agent_toml`, flat) | symlink (nested) | **managed copy** (flat) | symlink | symlink |
| Rule projection mode | `markdown_section_sync` | `markdown_section_sync` | `link_sync` | `markdown_section_sync` | `markdown_section_sync` |
| Rule target | `~/.codex/AGENTS.md` (managed block) | `~/.claude/CLAUDE.md` (managed block) | symlinks under `~/.cursor/rules/` | `~/.openclaw/workspace/SOUL.md` (managed block) | `~/.agents/AGENTS.md` (managed block) |
| Mirrored rule files | optional at `~/.codex/agentic-rules/` (annotated when present) | optional at `~/.claude/rules/` (rarely used) | n/a (rules are real files via symlink) | optional at `~/.openclaw/agentic-rules/` | optional at `~/.agents/rules/` |
| `commandLayout` | `Nested` | `Nested` | `Nested` | n/a | `Nested` |
| Command projection | symlink | **managed copy** (nested) | symlink | not supported | symlink |
| Command target | `~/.codex/prompts/` | `~/.claude/commands/` | `~/.cursor/commands/` | n/a | `~/.agents/commands/` |
| `hooks_enabled` default | `true` | `true` | `true` | `false` | `true` |
| `hooks_file` default | `~/.codex/hooks.json` | `~/.claude/settings.json` | `~/.cursor/hooks.json` | _none_ | `~/.agents/hooks.json` |
| Hook projection mode | `json_section` | `json_section` | `json_section` | not supported | `json_section` |
| Hook JSON shape | two-level (PascalCase events) | two-level (PascalCase events) | flat (camelCase events) | n/a | two-level (PascalCase events) |

### Codex is self-contained under `~/.codex`; OpenStandard owns `~/.agents`

The open-standard `~/.agents/` root (the convention OpenAI Codex documents for skills: `$HOME/.agents/skills`, walking up `$REPO_ROOT/.agents/skills`; see [Codex skills](https://developers.openai.com/codex/skills/)) is owned by its own first-class tool, **OpenStandard**, which projects skills, agents, rules, hooks, and commands under `~/.agents/`. Codex's own tool entry is now fully self-contained under `~/.codex/` (skills, agents, rules, instructions, hooks), so the two columns are independent. Codex **subagents** remain TOML files under `~/.codex/agents/` (user) and `.codex/agents/` (project), each with `name` / `description` / `developer_instructions` ([Codex subagents](https://developers.openai.com/codex/subagents)). OpenStandard is **global-only** (like OpenClaw, but enabled by default).

The Codex `agentsPath` default was `~/.agents/agents` before v0.5.0, which collided with the OpenStandard-owned shared root — Codex agent projection landed there instead of `~/.codex/agents`, where Codex actually reads subagents, so enabling Codex agents silently did nothing. Configs persisted before the default moved keep the stale path. `Settings::migrate_codex_agents_path()` (marker `codexAgentsPathMigrated`, run once from `setup()`) rewrites that exact superseded default to `~/.codex/agents`; a deliberate custom path is left untouched.

### Opt-in adapters

| Aspect | Kiro | GitHub Copilot | Google Antigravity |
|--------|------|----------------|--------------------|
| Tool id | `kiro` | `copilot` | `antigravity` |
| Default `enabled` | `false` | `false` | `false` |
| Skills | `~/.kiro/skills` | `~/.copilot/skills` | `~/.gemini/skills` |
| Agents | symlink to `~/.kiro/agents/*.md` | symlink to `~/.copilot/agents/*.agent.md` | not supported |
| Rules | symlink to `~/.kiro/steering` | symlink to `~/.copilot/instructions/*.instructions.md` | managed block in `~/.gemini/AGENTS.md` |
| Hooks | one v1 JSON per id in `~/.kiro/hooks` | one v1 JSON per id in `~/.copilot/hooks` | `json_section` in `~/.gemini/config/hooks.json` |
| Commands | not supported | not supported | not supported |
| Hook targeting | explicit `"kiro"` | explicit `"copilot"` | explicit `"antigravity"` |

### Codex agents project as transformed TOML, not a symlink

Codex loads subagents only from `*.toml` files (`name` / `description` / `developer_instructions`); a symlinked markdown spec is ignored. So Codex is the one tool whose **agent** projection is neither a plain symlink nor a verbatim managed copy: it uses the `codex_agent_toml` projection mode. The markdown source (YAML frontmatter `name` / `description` + body as `developer_instructions`) is rendered to a Codex subagent TOML (`crates/agentic-core/src/codex_agent.rs`) and written as a **managed copy** at `<name>.toml` (the adapter renames the `.md` source stem to `.toml`). Because the on-disk bytes are derived (not a byte-for-byte copy of the source), staleness is detected by re-rendering the expected TOML and comparing content, not by source hash. The applier carries the intent via `PlannedOperation.content_transform = CodexAgentToml`; absent that field, managed copies are written verbatim. On enable, any superseded `<name>.md` symlink the hub previously created is removed (self-heal); a user-authored `.md` at that path is never touched.

### Why Cursor agents and Claude skills are managed copies

Cursor loads agent files into memory at launch, and Claude's skill loader does not follow symlinks — for both, a symlink is unreliable. Managed copies are real files/folders recorded in a per-root `.agentic-hub-managed.json` manifest (`{ version, entries: { <relPath>: { itemId, sourcePath, sourceHash } } }`) that lets us detect drift (`stale` state) and explicitly refresh on user action. For skill folders the `sourceHash` is the `SKILL.md` hash.

### Why Claude commands are managed copies (and the others symlink)

Commands are file-based, nested markdown (`<root>/commands/**/*.md`), projected into each tool's slash-command directory. Cursor (`~/.cursor/commands`), Codex (`~/.codex/prompts`), and OpenStandard (`~/.agents/commands`) follow symlinks, so commands symlink there like skills/agents. Claude's command loader, like its skill loader, does **not** follow symlinks, so Claude commands are managed copies (`~/.claude/commands`). OpenClaw has no command concept and is unsupported. Commands always keep their nested path (no flat collapse).

### Which `(tool, kind)` flatten, and why

Flat layout collapses an item's `relative_path` to its basename; nested preserves the source folder structure. **Flatten wherever a tool's loader is non-recursive** (it scans only the top level of the target dir, so a nested file is never discovered):

| `(tool, kind)` | Layout | Why |
|----------------|--------|-----|
| Claude **skill** | Flat | Skill loader scans only the top level of `~/.claude/skills/` ([docs](https://code.claude.com/docs/en/skills); issues [#18192](https://github.com/anthropics/claude-code/issues/18192) / [#10238](https://github.com/anthropics/claude-code/issues/10238)). |
| Cursor **agent** | Flat | Subagent loader scans only the top level of `~/.cursor/agents/`; identity is the filename ([Cursor subagents](https://cursor.com/docs/subagents)). |
| Codex **agent** | Flat | Only top-level `*.toml` are loaded from `~/.codex/agents/`; nested files are ignored ([Codex subagents](https://developers.openai.com/codex/subagents)). |
| Claude **agent** | Nested | Agent loader walks subfolders **recursively**; identity is the `name` frontmatter, not the path ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)) — flattening would collide same-basename agents. |
| everything else | Nested | Loaders follow nested paths (or the path is the identity, e.g. commands). |

Flattening can map two differently-nested sources to the same basename (`zoom/cto.md` and `team/cto.md` → `cto.md`). Those collisions are resolved deterministically by the planner's `resolve_target_collisions` (lowest `item_id` wins; the rest become `skip_conflict`), exactly as for Claude skills. When the layout itself changes for an upgrading user (a previously-nested agent now projects flat), the applier self-heals: writing the flat managed copy prunes any copy of the *same item* left at the old nested path (`managed_copy::prune_other_paths_for_item`), including the now-empty folder. Only our own manifest-tracked copies are touched. See [claude-flat-skill-layout.md](../modules/claude-flat-skill-layout.md).

## Workspace scope (writes into project directories)

OpenClaw is **not supported** in workspace scope.

Workspace scope is **read-only inventory** (`workspace_inventory`): it scans these per-tool dirs and reports what each tool already has. It never writes. The "scan source" column is the directory/file each tool actually reads.

| Aspect | Codex | Claude Code | Cursor | Kiro | Copilot | Antigravity |
|--------|-------|-------------|--------|------|---------|-------------|
| Skills scan source | `<ws>/.agents/skills` | `<ws>/.claude/skills` | `<ws>/.cursor/skills` + `<ws>/.agents/skills` | `<ws>/.kiro/skills` | `<ws>/.github/skills` + `<ws>/.agents/skills` | `<ws>/.agents/skills` + `<ws>/.agent/skills` |
| Agents scan source | `<ws>/.codex/agents/*.toml` | `<ws>/.claude/agents/*.md` | `<ws>/.cursor/agents` + `<ws>/.agents/agents` | `<ws>/.kiro/agents/*.md` | `<ws>/.github/agents/*.agent.md` | _none_ |
| Rules scan source | (in `AGENTS.md`) | (in `CLAUDE.md`) | `<ws>/.cursor/rules/*.mdc` | `<ws>/.kiro/steering/*.md` | `<ws>/.github/instructions/*.instructions.md` | `<ws>/.agents/rules` + `<ws>/.agent/rules` |
| Hooks inventory | aggregate file not decomposed | aggregate file not decomposed | aggregate file not decomposed | `<ws>/.kiro/hooks/*.json` | `<ws>/.github/hooks/*.json` | aggregate file not decomposed |
| Commands scan source | `<ws>/.codex/prompts/**/*.md` | `<ws>/.claude/commands/**/*.md` | `<ws>/.cursor/commands/**/*.md` | _none_ | _none_ | _none_ |
| Instructions scan source | `<ws>/AGENTS.md` | `<ws>/CLAUDE.md` | `<ws>/AGENTS.md` | _none_ | `<ws>/.github/copilot-instructions.md` | `<ws>/AGENTS.md` |

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

- Global Claude / OpenClaw / OpenStandard: symlink — **nested** (Claude scans `~/.claude/agents/` recursively; identity is the `name` frontmatter)
- Global Codex: managed copy rendered to TOML (`codex_agent_toml`) — **flat** (Codex loads only top-level `*.toml` subagents)
- Global Cursor: managed copy (per-root manifest) — **flat** (Cursor's subagent loader is non-recursive; identity is the filename)
- Flat collisions resolve via `resolve_target_collisions`; a layout change self-heals via `prune_other_paths_for_item`
- Workspace: read-only inventory (Codex reads `.codex/agents/*.toml`; Claude/Cursor read `*.md`)

### Rule

- Global Cursor: symlink under `~/.cursor/rules/`
- Global Codex / Claude / OpenClaw / OpenStandard: managed block in instruction file
- Workspace Cursor: hard copy under `<ws>/.cursor/rules/`
- Workspace Codex / Claude: managed block in `<ws>/AGENTS.md` / `<ws>/CLAUDE.md`

### Command

- Global Codex / Cursor / OpenStandard: symlink (nested) into `~/.codex/prompts`, `~/.cursor/commands`, `~/.agents/commands`
- Global Claude: managed copy (nested) into `~/.claude/commands` — Claude's loader does not follow symlinks
- Global OpenClaw: not supported (no command concept)
- Workspace: read-only inventory (nested `*.md` under each tool's commands dir; Codex uses `prompts`)

### Hook

- Global Codex / Claude / Cursor / OpenStandard: managed JSON entry in the tool's hooks file (`json_section`)
- Global OpenClaw: not supported (no public hook spec)
- **OpenStandard hooks are opt-in.** A hook's default target set is the trio `[Cursor, Claude, Codex]` (`HookManifest::effective_targets`), so OpenStandard receives a hook only when the hook's `hook.json` lists it explicitly (`"targets": ["openstandard"]`). Unlike skills/agents/rules — which project to OpenStandard by default — hooks do not, to keep the default `~/.agents/hooks.json` empty unless asked for.
- Workspace Codex / Claude / Cursor: managed JSON entry in `<ws>/.codex/hooks.json` / `<ws>/.claude/settings.json` / `<ws>/.cursor/hooks.json`
- Cursor uses a flat shape (camelCase events); Codex / Claude use a two-level shape (PascalCase events, marker on the matcher group). See [hook-projection-sync.md](../modules/hook-projection-sync.md).

## Operation kinds per projection mode

| Mode | Create | Update | Remove |
|------|--------|--------|--------|
| Symlink (`link_sync`) | `create_link` | `replace_link` | `remove_link` |
| Managed copy (`file_sync` with metadata) | `create_managed_copy` | `replace_managed_copy` | `remove_managed_copy` |
| Codex agent TOML (`codex_agent_toml`) | `create_managed_copy` + `content_transform` | `replace_managed_copy` + `content_transform` | `remove_managed_copy` |
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

        // Commands (OpenClaw has no command concept)
        (OpenClaw, Command, _)       => Err(CommandUnsupportedForTool),
        (Claude,   Command, _)       => ManagedCopy,  // loader does not follow symlinks
        (_,        Command, _)       => LinkSync,

        // Global scope
        (Codex,    Agent, Global)    => CodexAgentToml,  // renders TOML; Codex loads only *.toml
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
        (Claude, Skill, Global)          => Flat,  // skill loader is non-recursive
        (Cursor | Codex, Agent, Global)  => Flat,  // subagent loaders are non-recursive
        _                                => Nested, // incl. Claude agents (recursive loader)
    }
}
```

Workspace scope always uses `Nested` (users own those paths fully).

## Why this lives in one table

If a developer asks "where does enabling skill X for tool Y go?", the answer should be one lookup, not a hunt through three files. The table is the contract; the module docs explain the why.

## Open questions

- Should we ever expose `skillLayout` and `agentLayout` as user-configurable settings? Decision: no in v1; layout is tool-intrinsic
- Should we eventually support a seventh tool (Aider, Continue, etc.)? Adding one is mostly: pick a tool id, add settings defaults, declare layout + projection mode, regression-test the planner. **Kiro** (2026-06-29) and **OpenStandard** are the worked examples.

## Kiro (global scope)

| Aspect | Kiro |
|--------|------|
| Tool id | `kiro` |
| Default `enabled` | `false` |
| `skillsPath` | `~/.kiro/skills` |
| `agentsPath` | `~/.kiro/agents` |
| `rulesPath` | `~/.kiro/steering` |
| `hooksDir` | `~/.kiro/hooks` |
| Skill projection | symlink (nested) |
| Agent projection | symlink (**flat** `.md`) |
| Rule projection | symlink under steering (not managed block) |
| Hook projection | `kiro_hook_file` — one v1 JSON per hook id |
| Commands | not supported |
| Hook targets | opt-in via `"targets": ["kiro"]` |

See [kiro-tool-adapter.md](../modules/kiro-tool-adapter.md).
