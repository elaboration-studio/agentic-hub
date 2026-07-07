# Module: Local Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-06
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [docs/features/local-skill-usage-tracing.md](../../features/local-skill-usage-tracing.md)
Related Docs: [docs/tech/modules/hook-projection-sync.md](./hook-projection-sync.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Local usage tracing records terminal skill/tool execution events from enabled
agentic tools into an app-owned SQLite database and exposes aggregated skill
counts to the Manager matrix. It is local-only and separate from remote
Aptabase telemetry.

## Boundaries

- `agentic-core::usage_store` owns schema, migrations, redaction, resolution,
  insert, retention cleanup, and stats queries.
- `agentic-hub::usage_collector` owns the loopback HTTP collector lifecycle and
  maps HTTP payloads into core events.
- React reads aggregated stats over IPC. It never opens the DB directly.
- Hook scripts are lightweight emitters. They do not connect to the database,
  perform analytics, or block the agentic tool when collection fails.

## Storage

Default path:

```text
~/.agentic-hub/usage/trace.db
```

Tables:

| Table | Purpose |
| --- | --- |
| `schema_migrations` | Applied schema versions |
| `usage_events` | Normalized event records |

`usage_events` fields:

| Field | Notes |
| --- | --- |
| `event_id` | UUID-like id from payload or generated locally |
| `timestamp` | ISO 8601 timestamp |
| `source_tool` | Agentic tool id such as `codex`, `claude`, or `cursor` |
| `event_type` | Terminal events count: `PostToolUse`, `PostToolUseFailure`, `PostSkillUse`, `CommandPaletteUse`, and MCP wrapper completions |
| `tool_name` | Tool or MCP tool name when available |
| `skill_name` | Explicit skill name when available |
| `capability_id` | Resolved Agentic Hub id, e.g. `skill:root-cause-investigation` |
| `workspace` | Optional workspace path, redacted to a tildified value |
| `project` | Optional project/repo label |
| `success` | Boolean terminal success flag |
| `duration_ms` | Optional duration |
| `dedupe_hash` | Unique key used to ignore duplicate lifecycle emissions |
| `metadata_json` | Small allowlisted metadata only |

## Resolution rules

The core resolves a skill event against the currently scanned local skills:

1. Exact `capability_id` if it starts with `skill:` and exists in the scan.
2. Exact `skill_name` match against `CapabilityItem.name`.
3. Exact `skill_name` match against a skill's source-relative path without the
   leading `skill:` prefix.
4. Normalized slug match against `CapabilityItem.name`.

If no match exists, or if more than one skill matches at the same stage, the
event is stored with `capability_id = null` and excluded from visible counts.

Cursor- and Codex-specific prompt-submit events may produce `PostSkillUse` only when the
payload contains a single explicit skill reference such as
`$root-cause-investigation` or a `.../root-cause-investigation/SKILL.md` link.
Claude slash-command invocations produce `PostSkillUse` from `UserPromptExpansion`
when `expansion_type` is `slash_command` and `command_name` resolves to one local
skill. Claude prompt-submit and Codex/Cursor prompt-submit paths use the same
conservative explicit-reference rules. The collector extracts only the skill slug
and discards the raw prompt text. Ambiguous prompt references stay unresolved.

## Collector flow

```text
Agentic tool hook
  -> POST http://127.0.0.1:<collectorPort>/events
  -> token check
  -> payload parse
  -> normalize + redact
  -> resolve local skill id
  -> insert into SQLite
  -> 202 Accepted
```

The collector binds only to `127.0.0.1`. Requests without the configured token
return `401`. Malformed JSON returns `400`. Storage failures return `202` after
logging because hook failures must not block the agentic tool.

## Managed tracer hooks

When tracing is enabled, Agentic Hub projects a built-in tracer hook into
supported enabled tools using the same `_agenticHub` marker style as normal hook
projection. The hook command forwards stdin to the collector and exits `0`
whether the collector is available or not.

When tracing is disabled, the managed tracer hook is removed. User-authored hook
entries are preserved verbatim.

## IPC

| Command | Purpose |
| --- | --- |
| `cmd_usage_tracing_status` | Return whether tracing is enabled, whether the collector is running, DB path, port, and supported tools |
| `cmd_set_usage_tracing_enabled` | Toggle tracing, persist settings, start/stop collector, and sync managed tracer hooks |
| `cmd_sync_usage_tracer_hooks` | Reinstall managed tracer hooks and ensure the collector is running without toggling tracing |
| `cmd_query_usage_stats` | Return per-capability usage totals for the current scan |
| `cmd_record_command_palette_usage` | Record a palette command copy or paste against a command capability id |

## Failure modes

| Failure | Impact | Recovery |
| --- | --- | --- |
| Collector port unavailable | Tracing status shows stopped; hooks degrade | Change port or restart app |
| Invalid token | Event rejected | Reinstall managed tracer hooks |
| Malformed payload | Event ignored | Fix integration payload |
| Ambiguous skill name | Event stored, visible count unchanged | Rename or disambiguate skill |
| SQLite write error | Hook still continues | Surface status error in Config |

Config status also exposes stored, resolved, and unresolved event counts. A high
unresolved count with a running collector means hooks are arriving but payloads
do not contain a safely resolvable skill identity.

## Tests

- Migration creates required tables and indexes.
- Duplicate dedupe hashes are ignored.
- Terminal events increment stats; non-terminal events are stored but excluded.
- Skill resolution covers exact id, exact name, relative path, normalized slug,
  missing, and ambiguous cases.
- Redaction drops prompt/source/argument fields before persistence.
- Collector accepts valid tokens, rejects invalid tokens, survives malformed JSON,
  and does not block when storage fails.
- Cursor lower-camel hook event names are canonicalized before storage.
- Cursor, Codex, and Claude prompt-submit events count only single explicit skill
  references and do not persist raw prompt text.
- Claude `UserPromptExpansion` slash-command events count when `command_name`
  resolves to exactly one local skill.
- Command palette copy/paste records `CommandPaletteUse` for resolved command
  rows when local tracing is enabled.
