# Module: Local Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-08-03
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
the normalized skill or agent name and, when supplied, an exact capability path.
Accepted signals are Skill tool calls, Claude `UserPromptExpansion`, `$skill`,
catalog-validated `/skill` or `/agent` tokens, `@agent-*` mentions, validated
skill links or attachments, reads of validated `SKILL.md` paths, and reads of
agent markdown files under an `/agents/` path. Generic slash commands that do
not resolve in the catalog, ordinary files, images, arbitrary paths, and
free-form semantic inference are ignored.

The capability catalog combines:

1. managed global source scans;
2. tool-global installed skills and agents;
3. skills and agents discovered under the active repository roots.

Resolution compares canonical exact paths first, documented tool scope and
precedence second, and unique names last. Codex and Cursor prefer a unique
workspace match, then a unique global match. Claude prefers unique global, then
unique workspace. Codex discovers ancestor `.agents/skills` roots from `cwd` to
repository root. Claude discovers ancestor and nested `.claude/skills` roots.
Cursor uses all `workspace_roots` and discovers `.agents/skills`,
`.cursor/skills`, `.claude/skills`, and `.codex/skills`. Repository agents are
discovered under `.cursor/agents` / `.agents/agents` (Cursor), ancestor
`.claude/agents` (Claude), and ancestor `.codex/agents` / `.agents/agents`
(Codex). Repository roots come from the hook payload, not Agentic Hub's
saved-workspace list.

A tool-global installed item is what that tool actually executes for a name, so
when a configured source exposes the same kind and name the installed item is
that source's **projection**, never a rival candidate. This holds regardless of
projection form — Cursor and Codex symlink, Claude hard-copies, and any tool can
also hold an unmanaged copy — which is what keeps the three tools in sync. An
installed item with no same-named configured source is a genuine tool-only
resource and keeps its `installed::<tool>::` identity.

Each projection also records the canonical source file it points at, which
breaks ties when several configured sources expose the same leaf name. The origin
is established in order: symlink canonicalization, then the managed-copy
manifest's `source_path`, then its recorded `source_hash` matched against
candidate content. Matching never relies on the manifest's `item_id`, which goes
stale whenever a skill moves between source folders.

Ambiguity is not silently discarded. A catalog-required reference whose name is
unknown (`/health`, stray paths) is still dropped as noise, but a name the
catalog *does* know and cannot resolve uniquely is stored as an unresolved event
so the invocation is counted and later reconciliation can repair it.

One request fans out to one occurrence per distinct resolved skill. Unresolved
high-confidence dollar references may be retained for diagnostics, but
catalog-required slash/path candidates that do not resolve are dropped.
Existing unresolved rows are reconciled when exactly one catalog candidate
matches: workspace-scoped rows re-resolve against their recorded repository and
are skipped if it no longer exists, while global rows re-resolve against the
global catalog alone. Uncertain history is preserved unchanged.

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
  -> on curl failure: spool ~/.agentic-hub/usage/spool/{id}.{meta,body}
  -> token check
  -> payload parse
  -> extract high-confidence references in memory
  -> discover tool-aware global + repository catalog
  -> resolve and build per-skill invocation keys
  -> transactionally upsert the occurrence batch
  -> 202 Accepted

Collector startup + 30s timer
  -> drain spool (drop stale tokens; persist matching entries)
```

The collector binds only to `127.0.0.1`. Requests without the configured token
return `401`. Malformed JSON returns `400`. Storage failures return `202` after
logging because hook failures must not block the agentic tool. The managed
tracer script buffers stdin, uses a 1s curl timeout, and always exits `0`.

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

Statistics keeps local-day `todayTopCapabilities` separate from the selected
`UsageDateRange`. The Today tab consumes that independent result first; only
Activity, Top usage, and Unused use the selected range. Inventory is computed
from the Manager scan and remains available when tracing is disabled.

## Failure modes

| Failure | Impact | Recovery |
| --- | --- | --- |
| Collector port unavailable | Tracing status shows stopped; hooks spool payloads | Change port or restart app; spool drains on recovery |
| Collector health probe fails | Hourly recovery restarts it three times without hooks sync | Restart Agentic Hub after the single outage notification |
| Invalid token | Event rejected; spool lines with stale tokens discarded | Reinstall managed tracer hooks |
| Malformed payload | Event ignored | Fix integration payload |
| Ambiguous skill name | High-confidence dollar event stored, visible count unchanged; catalog-required slash/path dropped | Invoke through an exact skill path or rename it |
| SQLite write error | Hook still continues; spool entry retained for retry | Surface status error in Config |

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
