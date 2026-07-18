# Feature: Session Explorer

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-18
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [DESIGN.md](../../DESIGN.md)
Related Docs: [docs/tech/modules/session-explorer.md](../tech/modules/session-explorer.md), [docs/features/local-skill-usage-tracing.md](./local-skill-usage-tracing.md), [docs/features/global-installed-resources.md](./global-installed-resources.md)

## Why now

Agentic Hub already answers *what capabilities exist* and *which skills get
used*. It cannot answer *what have I actually been doing across my agentic
tools*. Every tool — Codex, Claude Code, Cursor — keeps a rich local history of
coding sessions, but each stores it in a different, opaque place and format.
There is no single surface to browse "every session I ran in this repo this
week, in any tool," reopen a past conversation to recover context, or see how
work is spread across tools. Session Explorer makes that local history a
first-class, browsable resource without sending anything off the machine.

## User story

As an Agentic Hub user, I want to browse and search the local session history of
Codex, Claude Code, and Cursor in one place — with a readable title, the
originating repo, and the full transcript on demand — so that I can track what I
worked on, recover context from a past session, and understand how my work is
spread across tools.

## Scope

### In scope

- A **Sessions** pane in the Resources panel, beside Tools and Skills.
- Read-only ingestion of local session history from three tools:
  - Claude Code — `~/.claude/projects/**/*.jsonl`
  - Codex — `~/.codex/sessions/**/rollout-*.jsonl` plus `~/.codex/session_index.jsonl`
  - Cursor — `globalStorage/state.vscdb` (`cursorDiskKV`: `composerData:*` and
    `bubbleId:<composerId>:*` rows). No per-workspace `state.vscdb`/`workspace.json`
    join turned out to be needed — each bubble carries its own `workspaceUris`.
- A local **metadata-only** SQLite index at `~/.agentic-hub/sessions/index.db`
  (title, tool, workspace/cwd, git branch, model, timestamps, message count,
  source path). Transcript text is never copied into the index. **Not yet
  built** — see Delivery slices; the shipped list re-derives this shape live.
- A session list with title, tool, workspace, last-activity time, and message
  count, filterable by tool and workspace and searchable over title/metadata.
- A date-range filter (`UsageDateRange`: Today/Last 7/30/90 days, All time),
  defaulting to **Today** (rolling 24 hours, not calendar-day), applied by each
  reader as early as possible (a cheap file mtime stat for Claude/Codex, before
  any content is read) so the default narrows actual read work, not just the
  displayed list. `Today` is a Sessions-only addition to the shared
  `UsageDateRange` enum — the Statistics dashboard's own range selector still
  offers only Last 7/30/90 days and All time.
- A read-only session detail view that reads the full transcript **on demand**
  from the original source file and renders it as role-attributed messages.
- Reveal-in-Finder and open-source-file actions, reusing existing open/reveal
  commands.
- Incremental re-index when a tool's session store changes on disk, via the
  existing source watcher.

### Out of scope

- Writing to, editing, deleting, or archiving any tool's session data. Session
  Explorer is strictly read-only over other tools' stores.
- Persisting transcript content, prompts, tool arguments, or code snippets into
  any Agentic Hub-owned database. The index holds allowlisted metadata only.
- Full-text search across transcript bodies. v1 searches titles and metadata;
  a body-level search index is a later, explicitly opt-in slice.
- Resuming or launching a session (`claude --resume`, `codex resume`). Deep-link
  resume is a later slice; v1 is a viewer.
- Remote sync, cloud history, or cross-machine session merging.
- Tools without a documented local session store (OpenClaw in v1).

## Experience

The Resources panel's left rail gains a third item, **Sessions** (beside Tools
and Skills). Selecting it shows a two-pane view: a session list on the left and
a read-only detail view on the right.

The **list** shows one row per session across all three tools, newest activity
first. Each row shows the session title, a tool badge (Codex / Claude / Cursor),
the workspace or repo it ran in, the last-activity timestamp, and the message
count. Titles come from the best available source per tool: Codex's own
`thread_name`, the first user prompt for Claude, and a derived first-prompt
snippet for Cursor. A tool filter and a workspace filter narrow the list; a
search box matches over titles and metadata; a **date-range selector**
(Today / Last 7 days / Last 30 days / Last 90 days / All time — the same
`UsageDateRange` type as Statistics, with `Today` added as a Sessions-only
option) defaults to **Today** (rolling 24 hours). The range isn't a cosmetic
filter: reading every tool's full local history on every open is genuinely
expensive (measured ~9.2s across 1,531 real sessions on one
profile), so the default narrows what each reader even reads, not just what the
list displays — see the tech doc's Reader flow section. Rows for tools that
are disabled in Config are hidden, matching how the rest of the app treats
disabled tools.

Selecting a row opens the **detail** view, which reads the full transcript on
demand from the original file and renders it as role-attributed message bubbles
(user / assistant / tool), with timestamps. Because content is read live, the
detail view always reflects the current on-disk session and nothing is copied
into an Agentic Hub database. A **Copy as Markdown** button renders the
already-loaded session (title, tool, workspace, branch, model, timestamps, and
every message under a role heading) as one Markdown document and writes it to
the system clipboard — purely client-side over data the detail view already
holds, no new IPC command. Two further actions are available: **Reveal** (show
the source file in Finder) and **Open** (open the source file in the preferred
editor), both routed through the existing server-side-validated open/reveal
commands.

The index builds in the background on first open and updates incrementally. When
a tool writes to its session store, the source watcher triggers a scoped
re-index of the affected tool, and the list refreshes in place. A first-run or
empty state explains that no local sessions were found for the enabled tools.

## Acceptance criteria

- [x] The Resources rail shows a **Sessions** item beside Tools and Skills, and selecting it renders the sessions two-pane view.
- [x] The list shows sessions from Claude Code, Codex, and Cursor with a title, tool badge, workspace, last-activity time, and message count.
- [x] Codex session titles use `thread_name` from `session_index.jsonl` when present.
- [x] Claude session titles derive from the first user message of the session.
- [x] Cursor session titles derive from the first bubble's text (the composer's own `text` field is a stale input-box draft, not reliably the first message — used only as a fallback).
- [x] Sessions belonging to a tool disabled in Config are not shown.
- [x] The tool filter and workspace filter narrow the visible list.
- [x] The search box matches sessions over title and metadata.
- [x] A date-range selector defaults to Today (rolling 24 hours); Claude/Codex skip a file's content (not just its display) when the file's mtime falls outside the selected range; Cursor excludes a composer once its resolved last-activity timestamp falls outside range.
- [x] Selecting a wider range (e.g. All time) still resolves and opens any session the narrower default excluded.
- [x] Selecting a session renders its full transcript as role-attributed messages read on demand from the source.
- [x] A Copy as Markdown button on the open session copies its full transcript (title, metadata, and every message under a role heading) to the clipboard as one Markdown document, disabled while the transcript is loading or empty.
- [ ] No transcript text, prompt, tool argument, or code snippet is written into `~/.agentic-hub/sessions/index.db`; the index holds allowlisted metadata only. **Not applicable yet** — the index doesn't exist; nothing is persisted anywhere today (list/detail hold everything in memory).
- [ ] Reveal opens the source file in Finder; Open opens it in the preferred editor, both via server-side-validated paths. **Not built yet** (slice V1.3).
- [x] Cursor's SQLite store is opened read-only (`SQLITE_OPEN_READ_ONLY`, no copy needed — SQLite WAL lets a read-only reader see a consistent snapshot) and is never written.
- [ ] When a tool's session store changes on disk, the list re-indexes the affected tool and refreshes without a manual reload. **Not built yet** (slice V1.3); the pane has a manual Refresh button today.
- [x] The pane shows an empty state when no local sessions exist for the enabled tools.

## Dependencies

- SQLite storage in `agentic-core` (same `rusqlite` dependency as usage tracing).
- Tauri IPC commands and watcher wiring in `agentic-hub`.
- Existing open/reveal path-validation commands (`cmd_open_path`, `cmd_reveal_path`).
- Existing source watcher for incremental re-index.
- Existing generated Rust-to-TS type pipeline (`ts-rs`).
- Per-tool enabled state from Settings.

## Delivery slices

| Slice | What ships | Status |
| ----- | ---------- | ------ |
| V1 | Codex + Claude readers, in-memory session list, on-demand detail view in the Sessions pane, Copy as Markdown | **Shipped** |
| V1.2 | Cursor reader (read-only, key-only-scan strategy over `cursorDiskKV`; no `workspace.json` join needed) | **Shipped** — pulled forward ahead of V1.1 on request, since Cursor is the primary daily tool |
| V1.1 | Metadata-only SQLite index + tool/workspace filters as persisted queries | Partially delivered: the **date-range default + mtime-based skip** (pulled forward, see Risks) already gives most of this slice's speed benefit without the persisted index. The index itself — for instant search/filter with zero re-reads — is not built |
| V1.3 | Reveal/open actions, watcher-driven incremental re-index | Not built |
| V2 | Opt-in full-text (body) search index; session resume deep-links | Not built |

## Risks and edge cases

- Each tool's on-disk format differs and is undocumented; parsing must be
  defensive and tolerate schema drift without crashing. Confirmed in practice:
  a null-value tombstone row and empty-text bubbles both appear in real Cursor
  data and must be skipped, not crash the reader.
- Cursor writes its SQLite store while running; reads must not contend with or
  corrupt a live database. Resolved: `SQLITE_OPEN_READ_ONLY` is sufficient — no
  copy needed, since SQLite WAL mode gives a read-only reader a consistent
  snapshot.
- Cursor's scale is the real risk, confirmed against a live profile: 1,274
  composers, 67,017 bubbles, ~474MB, ~8s to read every bubble's content. The
  shipped reader avoids this by reading only bubble *keys* for counts plus one
  representative bubble per composer for title/workspace/timestamps (~0.3–0.5s
  total); full bubble content is read only per-session, on demand, in the
  detail view. Without the planned metadata index (V1.1), this per-tool cost
  is still paid on every list load, not just once — the date-range default
  reduces it (see below) but doesn't eliminate it.
- Reading every tool's full history on every list open was measured as
  genuinely expensive, confirmed against real local data: `list_all_sessions`
  across all three tools took **~9.2s for 1,531 sessions at `AllTime`**,
  vs **~3.2s for 82 sessions at `Last7Days`** (then the default) — about a
  2.9x reduction. The default has since been narrowed further to **`Today`**
  (rolling 24 hours, `range_days` = 1) for a further, un-benchmarked but
  by-construction reduction — `Today` is a strict subset of `Last7Days`'s
  read set, so it can only read the same amount or less on any given day.
  Claude/Codex get most of the `AllTime`→`Last7Days` gain from a cheap
  `fs::metadata` mtime stat that skips a file's content entirely when it's
  outside range (not just a post-hoc filter on an already-parsed list).
  Cursor's remaining cost at `Last7Days` is dominated by parsing every
  composer's metadata JSON (~68MB across ~1,274 rows) before checking its
  resolved timestamp against
  the cutoff; skipping that parse for out-of-range composers too — using the
  same key-embedded-id trick already used for bubbles — is a known further
  optimization, not yet done because the current default already resolves the
  reported complaint.
- A source can be deleted (Claude/Codex) or a session key can stop resolving
  (Cursor, since there's no persisted index yet) between listing and detail
  read; the detail view degrades to a clear "session no longer available"
  state rather than crashing.
- Titles derived from first prompts can contain sensitive text; they are shown
  in the local UI only and are still metadata, not full transcript persistence.
- Workspace attribution depends on each tool recording a `cwd`/folder (or, for
  Cursor, a bubble's `workspaceUris`); sessions without one are grouped under
  an "unknown workspace" bucket — confirmed to happen for some older/short
  Cursor sessions in practice.
- Cursor's `message_count` is a raw bubble count, not the number of messages
  the detail view actually renders — empty-text bubbles (tool-only turns)
  count toward it but are skipped in the transcript. This is a known,
  disclosed gap, not a bug: fully parsing Cursor's tool-call/diff fields is out
  of scope for this slice.

## Metrics or signals

- Count of indexed sessions per tool.
- Distribution of sessions per workspace.
- Last-activity recency per tool (freshness of the local history).
- Index build/refresh duration as a performance signal.

## Open questions

- Should the list default to grouping by workspace, by tool, or a flat
  newest-first timeline? Leaning flat newest-first with filters.
- Should deleted-on-disk sessions be pruned from the index immediately on
  watcher signal, or kept as tombstones until the next full reindex?
- For V2 body search, should the full-text index live in a separate opt-in DB so
  the metadata index stays content-free?
