# Module: Session Explorer

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-18
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [docs/features/session-explorer.md](../../features/session-explorer.md)
Related Docs: [docs/tech/modules/local-usage-tracing.md](./local-usage-tracing.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md), [docs/tech/modules/watcher.md](./watcher.md)

## Purpose

Session Explorer reads the local session history of Codex, Claude Code, and
Cursor, normalizes each tool's on-disk format into one session model, and backs
a browsable, searchable list. Full transcript content is read on demand from
the original source and is never copied into an Agentic Hub database. The
module is local-only, read-only over other tools' stores, and independent of
remote telemetry.

## Implementation status

Shipped: Claude, Codex, and Cursor readers; `cmd_list_sessions` /
`cmd_get_session`; the Sessions pane (list + on-demand transcript detail,
Copy as Markdown) in the Resources panel.

Not yet built (see [session-explorer.md feature doc](../../features/session-explorer.md)
delivery slices V1.3–V2): the persisted metadata-only `index.db` (listing
currently re-reads every source on each call), watcher-driven incremental
re-index, `cmd_reindex_sessions` / `cmd_sessions_status`, and Reveal/Open
actions in the Sessions pane. The **Storage**, **Incremental re-index**, and
those two IPC commands below describe that planned index slice, not current
behavior — each section says which applies.

## Boundaries

- `agentic-core::sessions` owns per-tool readers, the normalized session model,
  index schema/migrations, incremental upsert, and list/detail queries.
- `agentic-hub` owns the IPC commands and the watcher wiring that triggers a
  scoped re-index when a tool's session store changes.
- Per-tool readers are the only code that touches another tool's store. They
  open files read-only; Cursor's SQLite is opened read-only (or over a copy) and
  never written.
- React reads normalized sessions and transcripts over IPC. It never opens a
  tool store or the index directly.

## Session sources

| Tool | Store | Format | Title source |
| --- | --- | --- | --- |
| `claude` | `~/.claude/projects/<cwd-slug>/<uuid>.jsonl` | One append-only JSONL per session; lines carry `type`, `sessionId`, `cwd`, `gitBranch`, `timestamp`, `message`, `version` | First `user` message content (with the `slug` field as a fallback) |
| `codex` | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` plus `~/.codex/session_index.jsonl` | One JSONL rollout per session; first line is `type: session_meta` (`id`, `cwd`, `model_provider`, `cli_version`, `source`). The index file maps `id -> {thread_name, updated_at}` | `thread_name` from `session_index.jsonl` |
| `cursor` | `globalStorage/state.vscdb` (`cursorDiskKV` KV table: `key TEXT, value BLOB`) | Every composer (session) and bubble (message) is its own row: `composerData:<composerId>` and `bubbleId:<composerId>:<bubbleId>`. `composerData`'s `conversationMap`/`fullConversationHeadersOnly` fields are present in the schema but empty in practice — real content lives only in the separate `bubbleId:*` rows | First bubble's own text (bubble `type: 1` = user), falling back to the composer's `text` field (its input-box draft, not reliably the first message), then "Untitled session" |

The Claude directory name is the session `cwd` with path separators replaced by
dashes; the reader recovers the workspace from the per-line `cwd` field rather
than by decoding the directory name.

Cursor's scale forced a different reading strategy than Claude/Codex: one real
profile had 1,274 composers and 67,017 bubbles totaling ~474MB, and reading
every bubble's `value` took ~8s. Instead, listing reads only bubble **keys**
(cheap: a `substr(key, 10, 36)` + `COUNT(*)`/`MIN(rowid)`/`MAX(rowid)` grouped
in SQL, ~0.3–0.5s total) to get `message_count` and one representative
earliest/latest bubble per composer for title/workspace/timestamps — no
`workspace.json` join is needed because each bubble already carries its own
`workspaceUris` (`file://` URI) and ISO 8601 `createdAt`. Full bubble content
is read only in `read_cursor_transcript`, scoped to one composer id, which is
what the on-demand detail view actually needs.

## Storage (planned — not yet built)

The rest of this section describes the metadata-only index slice from the
feature doc's delivery table, not current behavior. Today, `cmd_list_sessions`
re-derives this same shape in memory on every call instead of reading it from
a persisted database.

Default path:

```text
~/.agentic-hub/sessions/index.db
```

Tables:

| Table | Purpose |
| --- | --- |
| `schema_migrations` | Applied schema versions |
| `sessions` | Normalized session metadata, one row per session |

`sessions` fields:

| Field | Notes |
| --- | --- |
| `session_key` | Stable id: `<tool>:<native session id>` |
| `tool` | `codex`, `claude`, or `cursor` |
| `title` | Best-available human title; metadata only |
| `workspace` | Tildified `cwd`/folder path, or null (unknown workspace) |
| `git_branch` | Optional branch when the tool records it |
| `model` | Optional model/provider label when available |
| `started_at` | ISO 8601 first-activity timestamp |
| `updated_at` | ISO 8601 last-activity timestamp |
| `message_count` | Number of turns/messages counted at index time |
| `source_path` | Absolute path to the source file (JSONL) or a store+key locator (Cursor) |
| `source_mtime` | Source mtime used to skip unchanged files on re-index |
| `content_hash` | Optional cheap hash used to detect changed content |

The index stores **no** transcript text, prompt bodies, tool arguments, or code.
`title` and `workspace` are the only human-derived strings, and both are treated
as metadata surfaced in the local UI only.

## Reader flow (current)

```text
cmd_list_sessions(range = UsageDateRange, default in the UI = today):
  per tool, on every call:
    Claude/Codex: walk the session-file tree; skip a file via a cheap
      fs::metadata mtime stat before reading it when its mtime falls outside
      range (files are append-only, so mtime tracks last activity); otherwise
      parse its header/turns for metadata + title
    Cursor: key-only scan of bubbleId:* for message_count, plus one
      earliest/latest bubble value per composer for title/workspace/timestamps
      (see the Cursor row above); a composer is excluded once its resolved
      last-activity timestamp falls outside range — this narrows the parsed
      composerData set but, unlike Claude/Codex, does not yet skip the
      composerData parse itself (see Known further optimization below)
  -> normalize into SessionSummary { session_key, tool, title, workspace, ... }
  -> merge all tools, sort newest-updated first
  -> filter by enabled tools / workspace / query
```

There is no persisted index yet, so per-tool read cost is still paid on every
list load — `range` reduces that cost (via the mtime/timestamp skips above),
it doesn't eliminate it the way a persisted, incrementally-updated index would.

**Measured against real local data:** `list_all_sessions(AllTime)` across
Claude+Codex+Cursor took ~9.2s for 1,531 sessions on one profile;
`list_all_sessions(Last7Days)` took ~3.2s for 82 sessions. Claude/Codex's
mtime-skip accounts for most of that; Cursor's remaining `Last7Days` cost is
dominated by parsing every composer's metadata JSON (~68MB across ~1,274
rows) before checking its timestamp.

The UI default has since been narrowed further, from `Last7Days` to `Today`
(`UsageDateRange::Today`, `range_days` = 1 — a rolling 24 hours, not a
calendar-day reset). `Today` reuses the exact same mtime/timestamp-skip
mechanism as every other range variant (`range_days` → `range_cutoff_system_time`
/ `range_cutoff_iso8601` → the same per-tool skip logic above), so it needed no
new code path — only the enum variant and its two match arms
(`range_days` here; `range_sql_clause` in `usage_store.rs`, shared with
Statistics). Not separately benchmarked, but by construction `Today`'s read
set is a subset of `Last7Days`'s on any given day, so it can only be faster or
equal. `Today` is a Sessions-only addition to the shared `UsageDateRange`
enum — Statistics' own range selector still exposes only Last 7/30/90
days/All time.

**Known further optimization (not done):** `composerData` keys already embed
the composer id (`composerData:<id>`), the same way `bubbleId:<composerId>:*`
does. A cheap key-only scan could enumerate composer ids and check each
against the already-computed last-bubble timestamp *before* fetching that
composer's full JSON value, skipping the parse for every out-of-range
composer instead of only skipping it from the returned list. Deferred because
the current default already resolves the reported "costs a lot of search
time" complaint; worth revisiting if Cursor's cost at the default range is
still felt to be slow.

## Detail (on-demand transcript)

`cmd_get_session` takes a `sessionKey`, re-derives the current session list to
resolve it to a `(tool, source)` pair (there is no persisted index to look
this up in yet), then reads the transcript live:

```text
cmd_list_sessions() -> find matching session_key -> (tool, source_path)
  -> read the source:
       Claude/Codex: the session's JSONL file
       Cursor: every bubbleId:<composerId>:* row, ordered by rowid
  -> map native turns into SessionMessage { role, text, tool_name?, timestamp }
  -> return; nothing is written back anywhere
```

If the session key no longer resolves (source deleted since the last list) or
the source can't be opened, the command returns a typed
`session_source_unavailable` error and the UI shows a "session no longer
available" state. Cursor transcripts render only bubbles with non-empty
`text`; a bubble whose real content lives in tool-call/diff fields with an
empty text summary is skipped rather than guessed at, so a Cursor transcript
can show fewer turns than the session actually contained. Malformed or partial
rows elsewhere yield the messages that parse, never a crash.

**Copy as Markdown** (`src/lib/sessionMarkdown.ts`) is a pure client-side
formatter over the `SessionSummary` + `SessionMessage[]` already held by the
open detail view — no new IPC command, no re-read of the source. It renders a
`# title` heading, a metadata line (tool/workspace/branch/model/timestamps,
omitting absent fields), then each message under a `### <role>` heading (the
role-attributed tool name for tool turns), fencing tool-turn text as a code
block since it's raw/preformatted rather than prose. The button writes the
result via the existing `copyText` clipboard helper (`ipc.ts`) and is disabled
while the transcript is loading or has no readable messages.

## Read-only guarantees

- Readers open every source file read-only.
- Cursor's `state.vscdb` is opened with `SQLITE_OPEN_READ_ONLY` (no copy) —
  SQLite's WAL mode lets a read-only reader see a consistent snapshot without
  contending with a running Cursor's writes.
- No command in this module writes into `~/.claude`, `~/.codex`, or Cursor's
  application-support tree.

## Incremental re-index (planned — not yet built)

The source watcher already observes tool directories. Session Explorer extends
the watched set with each tool's session store and, on a change under a store,
triggers a scoped re-index of that tool only (not a full rescan). Re-index uses
`(source_path, source_mtime)` to skip unchanged entries, upserts changed ones,
and removes rows whose source disappeared. After a re-index, the module emits a
`sessions-changed` event so the list refreshes in place. See
[watcher.md](./watcher.md).

## IPC

| Command | Purpose | Status |
| --- | --- | --- |
| `cmd_list_sessions` | Return normalized session metadata for the enabled tools, filtered by tool/workspace/query/date-range (`range`, required, UI defaults to `today`) | Shipped |
| `cmd_get_session` | Read one session's full transcript on demand, resolving `sessionKey` via a fresh list | Shipped |
| `cmd_reindex_sessions` | Force a full or per-tool re-index (recovery fallback) | Planned (needs the index) |
| `cmd_sessions_status` | Return index path, per-tool indexed counts, and last-index time | Planned (needs the index) |

`cmd_open_path` and `cmd_reveal_path` are reused for the Open and Reveal
actions; Session Explorer adds no new open/reveal surface. Reveal/Open aren't
wired into the Sessions pane yet (feature doc slice V1.3).

## Failure modes

| Failure | Impact | Recovery |
| --- | --- | --- |
| Tool store missing (tool never run) | That tool contributes zero sessions | Ignored; empty is normal |
| Malformed JSONL line / bubble / composer row | That entry is skipped or partially parsed | Parse defensively; render what resolves |
| Cursor DB locked by running Cursor | Read-only open still succeeds via WAL | No action needed |
| Session key no longer resolves (source deleted since last list) | `cmd_get_session` returns `session_source_unavailable` | User re-opens the list |
| Very large Cursor store | Slow list load | Key-only scans + one edge-bubble per composer keep this to sub-second on a 67k-bubble profile; a persisted index (planned) removes even this cost |

## Tests

- Claude reader extracts session id, cwd, branch, first-user-message title, and
  message count from a fixture JSONL.
- Codex reader joins `session_index.jsonl` `thread_name` onto the rollout file's
  `session_meta` and falls back to first turn when the index lacks an entry.
- Cursor reader is tested against a temp SQLite fixture built with the real
  `cursorDiskKV` schema (`composerData:*` + `bubbleId:<composerId>:*` rows,
  including a null-value tombstone row and a bubbleless draft composer, as seen
  in a real profile): title prefers the first bubble's text over the
  composer's stale draft text, workspace comes from the first bubble's
  `workspaceUris`, `message_count` is a raw bubble count, and timestamps come
  from bubble `createdAt` with an epoch-millis composer-level fallback for
  bubbleless composers.
- A manual `#[ignore]` test (`list_cursor_sessions_reads_real_local_db`) reads
  the real local `state.vscdb` and asserts at least one session with a
  non-empty rendered transcript — run explicitly with `--ignored`, not part of
  the default suite.
- `range_days`/`range_cutoff_system_time`/`range_cutoff_iso8601` cover each
  `UsageDateRange` variant, including `AllTime` producing no cutoff.
- `mtime_within_range` covers a past cutoff (kept), a future cutoff (skipped),
  `None`/`AllTime` (always kept), and a missing file (kept, not silently
  dropped).
- `list_claude_sessions_at` / `list_codex_sessions_at` back-date one fixture
  file's real mtime 40 days via `File::set_modified` (stable since Rust 1.75,
  no new dependency) and assert `Last30Days` excludes it while `AllTime`
  includes it — a real proof of the skip, not just the pure-function logic.
- `list_cursor_sessions_at` asserts a `Last7Days` range excludes fixture
  sessions dated far in the past, while `AllTime` includes them.
- A manual `#[ignore]` test (`list_all_sessions_last_7_days_is_faster_than_all_time`)
  measures `list_all_sessions` at `AllTime` vs `Last7Days` against real local
  data and prints the elapsed time for both — the source of the benchmark
  numbers cited in the Reader flow section above.
- `cmd_get_session` returns role-attributed messages read from the source and a
  typed error when the session key doesn't resolve.
- Sessions for a disabled tool are excluded from `cmd_list_sessions`.
- `sessionToMarkdown` (`src/lib/sessionMarkdown.test.ts`) covers the full
  metadata + turn rendering, omitting absent optional metadata fields, fencing
  tool-turn text as a code block, and the empty-transcript placeholder.
