# Module: Kiro Tool Adapter

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-29
Depends On: [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/tech/modules/hook-projection-sync.md](./hook-projection-sync.md)
Related Docs: [docs/features/kiro-tool-adapter.md](../../features/kiro-tool-adapter.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Purpose

Document how Agentic Hub integrates with Amazon Kiro — paths, projection modes, hook transform, and workspace inventory.

## Kiro filesystem conventions (global)

```
~/.kiro/
  skills/           # Agent Skills (SKILL.md folders)
  agents/           # Custom agents (*.md with YAML frontmatter — IDE)
  steering/         # Steering rules (*.md)
  hooks/            # Hook JSON files (v1 schema, one file per hook id)
```

Workspace scope mirrors under `<ws>/.kiro/`.

## Projection strategy

| Capability kind | Projection | Target |
|-----------------|------------|--------|
| `skill` | `link_sync` (symlink) | `~/.kiro/skills/` |
| `agent` | `link_sync` (symlink, **flat**) | `~/.kiro/agents/<name>.md` |
| `rule` | `link_sync` (symlink) | `~/.kiro/steering/` |
| `hook` | `kiro_hook_file` | `~/.kiro/hooks/<hook-id>.json` |
| `command` | not supported | — |

Rules use `rulesPath` → `~/.kiro/steering`. There is no `instructionsPath` (Kiro has no single instruction file for rules).

## Settings defaults

Persisted under `tools.kiro` in `~/.agentic-hub/config.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `false` | Hidden until opted in (OpenClaw-style) |
| `skillsPath` | `~/.kiro/skills` | Symlink target for skills |
| `agentsPath` | `~/.kiro/agents` | Symlink target for agents |
| `rulesPath` | `~/.kiro/steering` | Symlink target for steering rules |
| `hooksEnabled` | `true` | When tool is enabled, hooks can project |
| `hooksDir` | `~/.kiro/hooks` | Per-hook JSON files (not `hooksFile`) |
| `commandsPath` | `null` | No command concept in IDE v1 |

Legacy configs without `tools.kiro` receive serde defaults via `default_kiro()`.

## Hook projection (`kiro_hook_file`)

Module: `kiro_hook_sync.rs`. One managed JSON file per hook id:

```json
{
  "version": "v1",
  "hooks": [
    {
      "name": "auto-format-after-edit",
      "trigger": "PostToolUse",
      "matcher": "Edit|Write",
      "action": { "type": "command", "command": "/abs/path/script.sh" },
      "timeout": 30,
      "enabled": true,
      "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "…", "version": 1 }
    }
  ]
}
```

Each canonical event in the source manifest becomes one entry in the `hooks` array. Unsupported events produce notes, not errors.

### Event mapping

| Canonical | Kiro `trigger` |
|-----------|----------------|
| `PreToolUse` | `PreToolUse` |
| `PostToolUse` | `PostToolUse` |
| `UserPromptSubmit` | `UserPromptSubmit` |
| `Stop` | `Stop` |
| `SessionStart` | `SessionStart` |
| `PostFileSave` | `PostFileSave` |
| Others | skipped with note |

### Hook targets

Kiro is **opt-in**: add `"kiro"` to a hook's `targets` array. Default targets remain `[cursor, claude, codex]`.

## Workspace inventory

When Kiro is in `WORKSPACE_TOOL_IDS`, scan:

| Path | Pattern |
|------|---------|
| `<ws>/.kiro/skills` | dirs with `SKILL.md` |
| `<ws>/.kiro/agents` | `*.md` |
| `<ws>/.kiro/steering` | `*.md`, `*.mdc` |
| `<ws>/.kiro/hooks` | `*.json` |

## Tests

- `adapter_registry`: Kiro projection modes and flat agent layout
- `kiro_hook_sync`: transform, inspect, stale, foreign file, multi-event
- `workspace_inventory`: `.kiro/` scan paths
- `api`: end-to-end hook sync with `"targets": ["kiro"]`
