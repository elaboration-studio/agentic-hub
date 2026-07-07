# Local Skill Usage Tracing

## Summary
Add an opt-in, local-only usage tracing feature for Agentic Hub that records skill/tool execution events into `~/.agentic-hub/usage/trace.db`, joins counts onto existing `CapabilityItem` rows, and shows a `Usage` column in the Manager matrix. Hovering the count shows per-agentic-tool totals, e.g. Codex vs Claude vs Cursor.

Important constraint: current hooks reliably observe tool lifecycle events, but first-class per-skill invocation events are still uneven across tools. Claude documents skill invocation behavior, while public requests exist for richer named skill usage telemetry in Claude/Codex. So v1 must count only events that explicitly provide or can safely resolve a skill name; unknown events are stored but not guessed into a skill count. Sources: [Codex hooks](https://developers.openai.com/codex/hooks), [Claude hooks](https://code.claude.com/docs/en/hooks), [Claude skills](https://code.claude.com/docs/en/skills), [Claude skill-usage request](https://github.com/anthropics/claude-code/issues/35319), [Codex skill-hook request](https://github.com/openai/codex/issues/17132).

## Key Changes
- Create branch `feat/local-skill-usage-tracing-20260706`; update docs first with `docs/features/local-skill-usage-tracing.md` and `docs/tech/modules/local-usage-tracing.md`.
- Add `UsageTracingConfig` under `Settings`, default off:
  `enabled`, `captureTools`, `retentionDays`, `collectorPort`, `collectorToken`.
- Add `agentic-core::usage_store` backed by SQLite via `rusqlite`.
  Use `~/.agentic-hub/usage/trace.db`; add migration table plus `usage_events`.
- Add a local loopback collector in the Tauri shell, started only when tracing is enabled.
  It binds `127.0.0.1`, requires the generated token, redacts/normalizes payloads, and calls core storage. Hook failures must never block the agentic tool.
- Add a managed built-in tracer hook entry for enabled tools using the existing hook projection style and `_agenticHub` marker.
  The hook command posts stdin JSON to the local collector with a short timeout and exits `0` on failure.
- Add IPC:
  `cmd_usage_tracing_status`, `cmd_set_usage_tracing_enabled`, `cmd_query_usage_stats`.
  Export generated TS types for `UsageStats`, `UsageToolBucket`, and `UsageTracingStatus`.
- Add a Zustand usage slice or extend `manager` narrowly:
  load usage stats alongside scan/inspect, keyed by `CapabilityItem.id`.
- Add a `Usage` table column after `Source`.
  For skill rows, show total count; hover shows per-tool buckets and last-used time. Non-skill rows show `—` in v1 unless the event resolves to a command/tool row.

## Data Contract
- Store terminal execution events only for counts: `PostToolUse`, `PostToolUseFailure`, future `PostSkillUse`, and MCP wrapper completions.
- Event fields:
  `event_id`, `timestamp`, `source_tool`, `event_type`, `tool_name`, `skill_name`, `capability_id`, `workspace`, `project`, `success`, `duration_ms`, `dedupe_hash`, `metadata_json`.
- Resolve `capability_id` by exact `skill:<relative_path>`, exact skill name, then normalized slug match within currently scanned local skills.
  If resolution is ambiguous or missing, keep the event with `capability_id = null`; do not increment any skill row.
- Deduplicate with `dedupe_hash`; ignore duplicate inserts.
- Do not persist raw prompts, source code, full tool arguments, or secrets.

## Test Plan
- Rust unit tests:
  SQLite migration, insert/query, dedupe, retention cleanup, skill-name resolution, ambiguous-name behavior, redaction.
- Rust integration tests:
  collector accepts valid token, rejects invalid token, never panics on malformed JSON, does not block on storage errors.
- Existing suites:
  `cargo test --workspace`, `cargo clippy --all-targets --all-features --locked -- -D warnings`.
- UI tests:
  manager store merges stats by item id; matrix renders total count; hover bucket displays Codex/Claude/Cursor counts.
  Run `pnpm test`; visually verify the matrix column after implementation.

## Assumptions
- V1 uses the existing Manager matrix, not a new analytics page.
- V1 installs Agentic Hub-managed tracer hooks for enabled local tools when the feature is enabled.
- V1 is local-only and separate from Aptabase telemetry.
- V1 prioritizes accurate counts over inferred counts; unknown skill usage is stored for later diagnostics, not shown as a false positive.
- Effort estimate: about 1 week human team time / about 30-60 minutes Arno agentic-system time, mainly because the storage and UI are small but hook payload differences need careful tests.
