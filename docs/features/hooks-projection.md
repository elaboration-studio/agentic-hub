# Feature: Hooks Projection

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-31
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md), [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md)
Related Docs: [docs/tech/modules/hook-projection-sync.md](../tech/modules/hook-projection-sync.md), [docs/tech/modules/rule-projection-sync.md](../tech/modules/rule-projection-sync.md), [docs/tech/reference/tool-adapter-matrix.md](../tech/reference/tool-adapter-matrix.md)

## Purpose

Let a user enable and disable shared agentic **hooks** from the Agentic Hub window and have them safely surface in each tool's native hook configuration file — Cursor's `hooks.json`, Claude Code's `settings.json`, and Codex's `hooks.json` — without ever overwriting hook entries the user authored by hand.

Hooks are the fourth capability kind, alongside `skill`, `agent`, and `rule`. They are the first kind to use the `json_section` projection mode (see [hook-projection-sync.md](../tech/modules/hook-projection-sync.md)).

## User goal

> I keep a small library of shell-callable hooks (auto-format on edit, secret-redaction before file read, audit log on shell execute). I want to enable each one for the AI tools that should run it, and never have my own hand-edited entries clobbered.

## What is a hook

A hook lives under `<source>/hooks/<hook-name>/hook.json`, optionally with sibling scripts. It declares one or more lifecycle events (canonical PascalCase names from the Claude/Codex schema), a single command to invoke, and the set of tools it should be projected into.

```json
{
  "$schema": "agentic-hub.hook.v1",
  "id": "auto-format-after-edit",
  "name": "Auto-format after edit",
  "description": "Run prettier on edited files after Edit/Write tool calls.",
  "events": [
    { "name": "PostToolUse", "matcher": "Edit|Write" }
  ],
  "command": "${HOOK_DIR}/script.sh",
  "timeout": 30,
  "loopLimit": 3,
  "targets": ["cursor", "claude", "codex"]
}
```

Substitution and defaults:

- `${HOOK_DIR}` resolves to the absolute path of the hook's containing folder at projection time. The scanner sets a hook's `source_path` to that folder.
- `targets` is optional and defaults to `["cursor", "claude", "codex"]`. OpenClaw is never a default target (no public hook spec).
- `loopLimit` (positive integer) is a Cursor-only safety knob that caps re-runs on the same triggering event; surfaced as `loop_limit` in `~/.cursor/hooks.json` and ignored by tools that do not model it.

## Tool coverage

| Tool | Default target file | Shape | Default `hooks_enabled` |
|------|--------------------|-------|-------------------------|
| Cursor | `~/.cursor/hooks.json` | `{ version: 1, hooks: { <camelCase>: [] } }` | `true` |
| Claude Code | `~/.claude/settings.json` | `{ hooks: { <PascalCase>: [] }, ...other }` | `true` |
| Codex | `~/.codex/hooks.json` | `{ hooks: { <PascalCase>: [] } }` | `true` |
| OpenClaw | (none) | n/a | `false` |

In workspace mode each file resolves to `<ws>/.cursor/hooks.json`, `<ws>/.claude/settings.json`, and `<ws>/.codex/hooks.json`.

## Co-existence contract

Every entry Agentic Hub writes carries an inline `_agenticHub` marker:

```json
{
  "command": "/abs/path/script.sh",
  "matcher": "Edit|Write",
  "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "<sha256 of hook.json>", "version": 1 }
}
```

CRUD rules:

- On every sync, the core reads the entire target JSON, partitions entries into **managed** (has `_agenticHub`) and **foreign** (does not), and rebuilds only the managed set.
- Foreign entries are written back verbatim; their order and surrounding keys (`permissions`, `env`, `theme`, etc.) are preserved.
- For Claude/Codex's two-level shape, the core always creates its **own matcher group** per hook. A user matcher group is never merged into.
- Writes are atomic (temp file + `rename`).
- Malformed target JSON surfaces a `broken` state for every hook in that file and blocks any write until repaired.

The marker key `_agenticHub` is preserved verbatim from the VS Code extension for migration parity.

## State semantics

| LinkState | Meaning |
|-----------|---------|
| `enabled` | A managed entry exists for this hook with a matching `sourceHash` |
| `disabled` | No managed entry exists for this hook in the target file |
| `stale` | A managed entry exists but its `sourceHash` no longer matches the source `hook.json`; apply refreshes it |
| `broken` | Target JSON is malformed or unreadable; apply will not touch it |
| `foreign_file` | Target path exists but is not a regular file (directory, special file) |

The projection mode is `json_section`; the projection kind tags emitted by the planner are `sync_json_section` and `clear_json_section`.

## Not-Targeted contract

When the focused tool is not in a hook's `targets` (or that tool's `hooks_enabled` is off), the hook item is **locked** in that tool's view with a `Not Targeted` chip; the inspector explains how to extend the hook's `targets` array.

- The plan payload is sanitized by a `filter_desired_enabled_for_tool` step **before** planning, so a stale toggle can never produce a silent no-op. Each dropped item yields one descriptive note in the apply result.
- Parent / bulk-toggle counts use an **applicable count** (the subset of children whose `targets` include the focused tool), not the raw child count — so a parent never flashes `2/2 enabled` then reverts.

## Operations

The plan/apply pipeline emits two operation kinds for hook items:

- `sync_json_section` — ensure the hook is present in the target file as a managed entry.
- `clear_json_section` — remove this hook's managed entry. Foreign entries on the same event/matcher are preserved.

A single tool apply batches all hook operations into one read–merge–write per target file.

## Workspace integration

None. Hook projection writes only in global scope. Workspace scope is a read-only
inventory (see [workspace-inventory.md](../tech/modules/workspace-inventory.md))
and never writes hook files; surfacing a project's installed hooks is a noted
follow-up.

## Demo scaffold

The bundled demo tree ships an `auto-format-after-edit` hook (`hook.json` + `script.sh`) so the end-to-end flow is dogfoodable on first run. See [agentic-demo-scaffold.md](./agentic-demo-scaffold.md).

## Acceptance criteria

- [ ] A valid `hook.json` under `<source>/hooks/<name>/` appears as a `hook` capability in inventory
- [ ] Enabling a hook for Cursor writes a managed entry under the camelCase event array in `~/.cursor/hooks.json`
- [ ] Enabling a hook for Claude/Codex writes a dedicated managed matcher group in the PascalCase event in their target file
- [ ] Foreign hook entries and non-`hooks` top-level keys are preserved byte-for-byte across a sync
- [ ] `${HOOK_DIR}` expands to the hook source folder's absolute path
- [ ] An unsupported `(event, tool)` combo produces a per-tool note, not an error
- [ ] A hook whose `targets` excludes the focused tool is locked with a `Not Targeted` chip and is never applied
- [ ] A malformed target JSON surfaces `broken` and blocks the write
- [ ] Disabling a hook removes only its managed entry; the file is deleted only when empty, hooks-only, and free of foreign keys
- [ ] Workspace apply writes the per-workspace files and records the `::managed-hooks` sentinel

## Out of scope

- Hook trust dialogs (Codex `/hooks` UI, Cursor team enforcement) — the user manages trust in each tool. Agentic Hub only *writes* the config entry; it never executes a hook (`tauri-plugin-shell` is never loaded).
- Plugin-bundled hooks (Codex `plugin.json`, Claude plugin hooks) — only the user-scope file is written.
- Two-way edit: if a user hand-edits a managed entry, the next sync overwrites it. Foreign entries remain untouched.

## References

- Cursor hooks: <https://cursor.com/docs/hooks.md>
- Claude Code hooks: <https://code.claude.com/docs/en/hooks.md>
- Codex hooks: <https://developers.openai.com/codex/hooks>
