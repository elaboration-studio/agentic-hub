# Module: Grok Build Tool Adapter

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-08-25
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/tech/modules/hook-projection-sync.md](./hook-projection-sync.md), [docs/tech/modules/local-usage-tracing.md](./local-usage-tracing.md)
Related Docs: [docs/features/grok-tool-adapter.md](../../features/grok-tool-adapter.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Purpose

Document how Agentic Hub integrates with Grok Build — paths, projection modes, the per-file Claude-style hook transform, workspace inventory, and usage-tracing attribution.

Source of truth for Grok's own contract: `~/.grok/docs/user-guide/10-hooks.md`, `08-skills.md`, `12-project-rules.md`, `16-subagents.md`.

## Grok filesystem conventions (global)

```
~/.grok/
  skills/           # SKILL.md folders; walked recursively
  agents/           # Custom agents (*.md with YAML frontmatter)
  rules/            # Home-level project rules (*.md)
  commands/         # Flat *.md slash commands (filename stem = name)
  hooks/            # One JSON file per hook source (always trusted)
```

Workspace scope mirrors under `<ws>/.grok/`. Project hooks require `/hooks-trust`; the hub never writes them.

Grok also scans vendor-compat trees (`~/.claude/skills`, `~/.cursor/skills`, `.agents/skills`) when those compat cells are on. The hub does **not** project into those trees for Grok — native `~/.grok/` is the Grok column's target.

## Projection strategy

| Capability kind | Projection | Target |
|-----------------|------------|--------|
| `skill` | **symlink** (nested) | `~/.grok/skills/` |
| `agent` | **symlink** (flat `*.md`) | `~/.grok/agents/` |
| `rule` | **symlink** (nested) | `~/.grok/rules/` |
| `command` | **symlink** (flat `*.md`) | `~/.grok/commands/` |
| `hook` | `grok_hook_file` | `~/.grok/hooks/<hook-id>.json` |

No `instructionsPath`. Home rules are files under `rules/`, like Cursor, not a managed AGENTS.md block. Project-root `AGENTS.md` remains a workspace-inventory attribution (Grok loads it) and is already scanned for other tools.

Grok walks skill directories recursively and does not document symlink rejection, so skills stay nested symlinks. Agents and commands are documented as files in those directories — flatten to basename. Rules keep nested paths (Cursor-like); flatten later if the loader is top-level-only.

## Settings defaults

Persisted under `tools.grok` in `~/.agentic-hub/config.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `false` | Hidden until opted in |
| `skillsPath` | `~/.grok/skills` | Nested skill symlinks |
| `agentsPath` | `~/.grok/agents` | Flat agent symlinks |
| `rulesPath` | `~/.grok/rules` | Nested rule symlinks |
| `instructionsPath` | `null` | File-based rules; no managed block |
| `hooksEnabled` | `true` | When the tool is enabled, hooks can project |
| `hooksDir` | `~/.grok/hooks` | Per-hook JSON files (not `hooksFile`) |
| `commandsPath` | `~/.grok/commands` | Flat command markdown |

Legacy configs without `tools.grok` receive serde defaults via `default_grok()`.

## Hook projection (`GrokHookFile`)

Module: `grok_hook_sync.rs`. One managed JSON file per hook id. Schema matches Grok's Claude-compatible two-level object, **not** Kiro v1 and **not** Copilot camelCase:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          { "type": "command", "command": "/abs/path/script.sh", "timeout": 30 }
        ],
        "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "…", "version": 1 }
      }
    ]
  }
}
```

- Event keys are PascalCase (Grok's native names). Cursor camelCase is accepted by Grok when loading `~/.cursor/hooks.json`; hub-written Grok files use PascalCase.
- The `_agenticHub` marker sits on the matcher group, same as Claude/Codex `json_section`.
- Each managed hook owns its file. Apply creates a missing file or replaces a fully hub-managed file. Directories, foreign entries, unmarked files, and malformed JSON are refused.
- Removal deletes only files whose entries are fully hub-managed for that hook id.
- `${HOOK_DIR}` expands at write time.
- HTTP `type` is never written; hub hooks are always `type: "command"`.

### Event mapping

| Canonical | Grok event key |
|-----------|----------------|
| `PreToolUse` | `PreToolUse` |
| `PostToolUse` | `PostToolUse` |
| `PostToolUseFailure` | `PostToolUseFailure` |
| `UserPromptSubmit` | `UserPromptSubmit` |
| `Stop` | `Stop` |
| `SessionStart` | `SessionStart` |
| `SessionEnd` | `SessionEnd` |
| `PreCompact` | `PreCompact` |
| `PostCompact` | `PostCompact` |
| `Notification` | `Notification` |
| `UserPromptExpansion` | skipped with note |
| `PermissionRequest` | skipped with note (Grok emits `PermissionDenied`, a different event) |

Unsupported events produce notes, not errors.

### Hook targets

Grok is **opt-in**: add `"grok"` to a hook's `targets` array. Default targets remain `[cursor, claude, codex]`. The usage tracer manifest sets `targets: ["grok"]` for its Grok copy.

## Usage tracing

When tracing is enabled and Grok is enabled, `internal_hooks` includes Grok in `USAGE_TRACER_TOOLS`. The collector installs the tracer via `grok_hook_sync::sync_single_grok_hook`, same path as Kiro's per-file install.

Tracer events: `UserPromptSubmit`, `PostToolUse`, `PostToolUseFailure`. Command is the shared `usage-tracer.sh` with `source_tool=grok`. Timeout 2s. Always exit 0.

Existing configs that omit `grok` from `capture_tools` still capture an enabled Grok tool (same fallback as Kiro).

### Payload contract (Grok stdin)

Grok sends camelCase JSON on stdin. Common fields: `hookEventName`, `sessionId`, `cwd`, `workspaceRoot`, `timestamp`, `permissionMode`, `promptId` (absent on session-scoped events), plus event-specific `toolName`, `toolInput`, `toolResult`.

| Grok field | Hub handling |
|------------|--------------|
| `hookEventName`: `post_tool_use`, `post_tool_use_failure`, `user_prompt_submit`, `pre_tool_use` | Canonicalize to PascalCase before storage |
| `promptId` | Turn correlation (like Claude `promptId`) |
| `workspaceRoot`, `cwd` | Workspace roots for catalog discovery |
| `toolName`: `read_file` (alias `Read`) | Treat as a skill/agent file read |
| `toolInput.target_file` | Path for `SKILL.md` / `/agents/` reads |
| `prompt` / `userPrompt` on `UserPromptSubmit` | Slash, `$skill`, and skill-link extraction |

Do not persist `toolInput`, `toolResult`, prompts, or `sessionId`.

### Catalog roots

For `source_tool=grok`:

1. Tool-global installed: `settings.tools.grok.skillsPath` (default `~/.grok/skills`) and `agentsPath`.
2. Repository: walk ancestors of each workspace root for `.grok/skills`, `.agents/skills`, `.grok/agents`, `.agents/agents`.
3. Precedence: unique workspace match, then unique global match (Codex/Cursor/Kiro style). Grok's own priority is CWD `.grok` > repo `.grok` > user `~/.grok`.
4. Qualified slash tokens `/user:name`, `/local:name`, `/repo:name`, `/<plugin>:name` strip the first `prefix:` before catalog match.

Compat Claude/Cursor skill dirs are not extra catalog roots. If the same skill is also projected to Claude, the Grok-installed copy (or unique name) wins; the occurrence still records `source_tool=grok`.

## Workspace inventory

When Grok is in `WORKSPACE_TOOL_IDS`, scan:

| Path | Pattern |
|------|---------|
| `<ws>/.grok/skills` | dirs with `SKILL.md` |
| `<ws>/.grok/agents` | `*.md` |
| `<ws>/.grok/rules` | `*.md`, `*.mdc` |
| `<ws>/.grok/hooks` | `*.json` |
| `<ws>/.grok/commands` | `*.md` |
| `<ws>/AGENTS.md` | already attributed to tools that read it; include Grok |

No writes.

## Integration points

| System | Interaction |
|--------|-------------|
| `model::ToolId` | add `Grok`; `ALL` length 9 |
| `settings::ToolsSettings` | `grok` with `#[serde(default = "default_grok")]` |
| `adapter_registry` | paths, `GrokHookFile`, nested skill / flat agent / nested rule / flat command, workspace adapter |
| `grok_hook_sync` | inspect + sync + `sync_single_grok_hook` |
| `api::sync_hooks_for_adapter` / `inspect_hooks_for_adapter` | dispatch `GrokHookFile` |
| `planner` | skip `GrokHookFile` like Kiro/Copilot (hooks not planned as FS ops) |
| `internal_hooks` | include Grok in `USAGE_TRACER_TOOLS` |
| `usage_collector/hooks.rs` | route Grok tracer install through `sync_single_grok_hook` |
| `usage_attribution` | snake_case events, `promptId`, `read_file`, qualified slash strip |
| `usage_catalog` | Grok skill/agent roots |
| UI `TOOL_LABELS`, Config accordion, `toolTargets.ts` | `grok` label and path rows |
| `ts-rs` codegen | regenerate `ToolId` |

## Failure modes

| Failure | Impact | Recovery |
|---------|--------|----------|
| Target hook file is a directory | `foreign_file`; skip | Move/remove; re-apply |
| JSON parse error | `broken`; refuse overwrite | Fix or delete the file |
| User-authored hook in same filename | refuse overwrite | Rename foreign file or change hub hook id |
| Grok not enabled | no projection, no tracer | Enable in Config |
| Collector down | tracer spools; Grok continues (exit 0) | Restart Agentic Hub; spool drains |
| Snake_case event not canonicalized | events stored as unknown; no skill count | Canonicalize `post_tool_use` etc. |

## Tests

- `adapter_registry`: Grok projection modes and layouts; workspace adapter paths
- `settings`: missing `tools.grok` loads defaults; Grok disabled by default
- `grok_hook_sync`: two-level PascalCase write, marker, stale, foreign file, unsupported event notes, tracer-shaped multi-event file
- `api`: hook sync with `"targets": ["grok"]`; default-target hooks skip Grok
- `workspace_inventory`: `.grok/` scan paths
- `internal_hooks` / collector: tracer enabled for Grok when tool + tracing on; removed when either off
- Attribution fixtures: snake_case `post_tool_use`, `read_file` + `target_file` SKILL.md, `/user:commit` prefix strip, `promptId` dedupe, `.grok/skills` workspace catalog
- UI store tests: Grok in labels / workspace tool set / Config tool list

## Open questions

- Confirm in dogfooding whether `~/.grok/rules/` is recursive. Nested until proven otherwise.
