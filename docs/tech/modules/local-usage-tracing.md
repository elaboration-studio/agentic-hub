# Module: Local Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-19
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [docs/features/local-skill-usage-tracing.md](../../features/local-skill-usage-tracing.md)
Related Docs: [docs/tech/modules/hook-projection-sync.md](./hook-projection-sync.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Local usage tracing attributes explicit skill invocations from enabled local
agentic tools into an app-owned SQLite database and exposes scoped aggregates
to Manager and Statistics. It supports global and repository-local skills,
multiple skills in one turn, and remains separate from remote Aptabase
telemetry.

## Boundaries

- `agentic-core::usage_store` owns schema, migrations, transactional batch
  insertion, reconciliation, retention cleanup, and scope-aware queries.
- `agentic-hub::usage_collector` owns HTTP and health lifecycle; focused sibling
  modules own extraction, normalization, catalog discovery, attribution, and
  in-memory turn correlation.
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

Schema v2 keeps all v1 fields and adds:

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
| `capability_scope` | `global` or `workspace` |
| `workspace_root` | Canonical, tildified repository root for workspace events |
| `capability_relative_path` | Stable path from the capability scope root |
| `invocation_key` | Privacy-safe once-per-capability-per-turn key |
| `attribution_source` | High-confidence signal class, never raw payload data |
| `attribution_rank` | Signal confidence used to upgrade fallback rows |

`invocation_key` has a unique partial index when non-null. The legacy
`dedupe_hash` uniqueness rule remains for v1 compatibility. Raw prompts, tool
arguments, session ids, turn ids, and skill contents are never stored.

## Attribution contract

Normalization emits a typed batch of explicit references. Each reference keeps
the normalized skill name and, when supplied, an exact `SKILL.md` path. Accepted
signals are Skill tool calls, Claude `UserPromptExpansion`, `$skill`, validated
skill links or attachments, validated slash-skill names, and reads of validated
`SKILL.md` paths. Generic slash commands, ordinary files, images, arbitrary
paths, and free-form semantic inference are ignored.

The capability catalog combines:

1. managed global source scans;
2. tool-global installed skills;
3. skills discovered under the active repository roots.

Resolution compares canonical exact paths first, documented tool scope and
precedence second, and unique names last. Codex discovers ancestor
`.agents/skills` roots from `cwd` to repository root. Claude discovers ancestor
and nested `.claude/skills` roots and applies its scope precedence. Cursor uses
all `workspace_roots` and discovers `.agents/skills`, `.cursor/skills`, and its
documented compatible nested roots. Repository roots come from the hook payload,
not Agentic Hub's saved-workspace list.

One request fans out to one occurrence per distinct resolved skill. Unresolved
high-confidence references may be retained for diagnostics, but unvalidated
slash/path candidates are dropped. Existing unresolved rows are reconciled only
when their recorded workspace still exists and exactly one catalog candidate
matches; uncertain history is preserved unchanged.

## Turn identity and deduplication

- Cursor correlation uses `generation_id`.
- Codex correlation uses `turn_id`.
- Claude uses `prompt_id` when present. For installed versions without it, an
  in-memory per-session tracker advances on prompt submission and associates
  expansion/tool signals with that turn.

The collector hashes correlation material in memory and combines it with tool,
workspace scope, and capability identity to create `invocation_key`. A skill is
therefore counted once per turn while different skills in the same turn keep
independent rows. A higher-rank terminal signal updates an existing lower-rank
prompt fallback rather than inserting another count.

## Collector flow

```text
Agentic tool hook
  -> POST http://127.0.0.1:<collectorPort>/events
  -> token check
  -> payload parse
  -> extract high-confidence references in memory
  -> discover tool-aware global + repository catalog
  -> resolve and build per-skill invocation keys
  -> transactionally upsert the occurrence batch
  -> 202 Accepted
```

The collector binds only to `127.0.0.1`. Requests without the configured token
return `401`. Malformed JSON returns `400`. Storage failures return `202` after
logging because hook failures must not block the agentic tool.

## Health and recovery

`GET /health` is a loopback-only, token-authenticated endpoint that returns
`204 No Content` and never writes an event. `collectorRunning` means this probe
succeeds within its short timeout; it does not mean merely that the app still
holds a shutdown sender.

The app owns one hourly health task for its lifetime. It skips disabled tracing.
For an unhealthy enabled collector, it reloads settings before each of three
restart-and-probe attempts, waiting five seconds between attempts. Recovery only
restarts the collector; it never rewrites managed tracer hooks. A successful
probe clears the current outage. Three failed attempts emit one
`usage-tracing-health-failed` event for that outage, which the main window turns
into a native desktop notification and persistent restart prompt. Disabling
tracing clears the outage state. A fully quit app cannot run this task.

## Managed tracer hooks

When tracing is enabled, Agentic Hub projects a built-in tracer hook into
supported enabled tools using the same `_agenticHub` marker style as normal hook
projection. The hook command forwards stdin to the collector and exits `0`
whether the collector is available or not.

Tracer hooks are also registered as virtual `Agentic Hub` hook rows in the
Manager. They are visible for inspection and included in hook sync payloads, but
cannot be toggled from the Manager or palette; Config remains the only control
surface for enabling/disabling tracing.

When tracing is disabled, the managed tracer hook is removed. User-authored hook
entries are preserved verbatim.

## IPC

| Command | Purpose |
| --- | --- |
| `cmd_usage_tracing_status` | Return tracing and authenticated collector health plus per-tool hook-installed, last-event, resolved, and unresolved diagnostics |
| `cmd_set_usage_tracing_enabled` | Toggle tracing, persist settings, verify collector health, and sync managed tracer hooks |
| `cmd_sync_usage_tracer_hooks` | Reinstall managed tracer hooks and recover an unhealthy collector without toggling tracing |
| `cmd_query_usage_stats` | Return scoped per-capability totals mapped to global ids or transient `ws::` Manager ids |
| `cmd_query_usage_dashboard` | Return workspace-aware dashboard metrics for Statistics (`UsageDashboard`, filtered by `UsageDateRange`) |
| `cmd_record_command_palette_usage` | Record a palette command copy or paste against a command capability id |

## Failure modes

| Failure | Impact | Recovery |
| --- | --- | --- |
| Collector port unavailable | Tracing status shows stopped; hooks degrade | Change port or restart app |
| Collector health probe fails | Hourly recovery restarts it three times without hooks sync | Restart Agentic Hub after the single outage notification |
| Invalid token | Event rejected | Reinstall managed tracer hooks |
| Malformed payload | Event ignored | Fix integration payload |
| Ambiguous skill name | High-confidence event stored, visible count unchanged | Invoke through an exact skill path or rename it |
| SQLite write error | Hook still continues | Surface status error in Config |

A high unresolved count with a running collector usually means hooks are
arriving with a skill reference that could not be matched to one local
capability. Generic tool calls without a skill signal are no longer stored.

## Tests

- v1-to-v2 migration preserves history and creates scoped fields and indexes.
- Batch insertion is transactional; duplicate invocation keys upgrade only when attribution rank increases.
- Legacy duplicate dedupe hashes remain ignored.
- Terminal events increment stats; non-terminal events are stored but excluded.
- Skill resolution covers exact id, exact name, relative path, normalized slug,
  missing, and ambiguous cases.
- Redaction drops prompt/source/argument fields before persistence.
- Collector accepts valid tokens, rejects invalid tokens, survives malformed JSON,
  and does not block when storage fails.
- Health accepts only the configured token, records no event, and makes status
  report stopped when the listener is unreachable.
- Recovery retries a stopped collector three times, clears the outage after a
  successful probe, and emits only one failure event per continuous outage.
- Cursor lower-camel hook event names are canonicalized before storage.
- Cursor, Codex, and Claude fixtures extract every distinct explicit skill in a
  turn and never persist raw prompt or tool input.
- `/health`, images, pasted files, arbitrary paths, traversal, missing paths,
  and out-of-root paths produce no occurrence.
- Attribution covers repository-only, nested, symlinked, global-only,
  same-name global/local, Cursor multi-root, and exact-path cases.
- Claude `UserPromptExpansion` and older-version session tracking deduplicate
  prompt and terminal signals.
- Statistics and Manager query mapping keep same-named global/workspace skills
  separate and preserve historical workspace context.
- Command palette copy/paste records `CommandPaletteUse` for resolved command
  rows when local tracing is enabled.
