# Feature: Local Skill Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-06
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [DESIGN.md](../../DESIGN.md)
Related Docs: [docs/tech/modules/local-usage-tracing.md](../tech/modules/local-usage-tracing.md), [docs/features/hooks-projection.md](./hooks-projection.md)

## Why now

Agentic Hub can show which skills are installed and where they are projected,
but it cannot answer whether a skill is actually used. As local skill libraries
grow, users need a local, privacy-preserving way to see which skills are active
across Codex, Claude Code, Cursor, and other enabled tools.

## User story

As an Agentic Hub user, I want to see how often each local skill is invoked and
which agentic tools invoked it, so that I can prune unused skills and understand
which capabilities are worth maintaining.

## Scope

### In scope

- Opt-in local usage tracing, disabled by default.
- Local SQLite persistence under `~/.agentic-hub/usage/trace.db`.
- Loopback-only event collection from managed tracer hooks.
- Skill usage counts joined onto existing Manager matrix rows.
- Agent spec usage counts joined onto existing Manager matrix rows when invoked
  via explicit slash (`/cto`), Claude `@agent-*` mention, or a single Read of an
  agent markdown file under an `/agents/` path.
- A `Usage` table column with hover details by source tool.
- Accurate-only skill attribution: unresolved or ambiguous events are stored but
  do not increment a visible skill count.

### Out of scope

- Remote sync or cloud analytics.
- A standalone analytics dashboard.
- Inferring skill usage from free-form prompts, transcripts, or raw source code.
  Cursor and Codex v1 may count an explicit skill reference such as `$root-cause-investigation`
  or a `SKILL.md` link from prompt-submit hook input; Claude slash commands count via
  `UserPromptExpansion.command_name`; the raw prompt is never
  persisted.
- Editing or deleting usage events from the UI.
- Non-skill capability analytics beyond storing raw terminal events.

## Experience

The Config page exposes a local usage tracing toggle. When enabled, Agentic Hub
starts a loopback collector and projects a managed tracer hook into supported,
enabled tools. Hooks forward terminal execution events to the local collector
with a short timeout; hook failures never block the calling agentic tool.
Config also shows stored, resolved, and unresolved local event counts so users
can distinguish collection failures from attribution gaps.

The Manager matrix adds a `Usage` column after `Source`. Skill, agent, and
command rows show the total attributed execution count. Hovering the number
shows per-tool counts (or `Palette` for command palette usage) and the last-used
timestamp. Rule and hook rows show `-` in v1.

## Acceptance criteria

- [ ] Existing settings files load with tracing disabled by default.
- [ ] Enabling tracing starts the local collector and installs managed tracer hooks for supported enabled tools.
- [ ] Disabling tracing stops the collector and removes managed tracer hooks.
- [ ] A valid terminal event with a resolvable skill or agent name is stored and increments that row.
- [ ] Duplicate events with the same dedupe hash are ignored.
- [ ] Events with missing or ambiguous skill names are stored without incrementing any skill row.
- [ ] The Manager matrix shows a `Usage` column after `Source`.
- [ ] Hovering a skill's usage count shows per-tool counts.
- [ ] Command palette copy/paste increments usage for the matching command row when tracing is enabled.
- [ ] No raw prompts, source snippets, or tool arguments are persisted.
- [ ] Config shows stored/resolved/unresolved event diagnostics.

## Dependencies

- SQLite storage in `agentic-core`.
- Tauri collector and IPC commands in `agentic-hub`.
- Existing hook projection conventions and `_agenticHub` marker semantics.
- Existing generated Rust-to-TS type pipeline.

## Delivery slices

| Slice | What ships | Why this cut |
| ----- | ---------- | ------------ |
| V1 | Local store, collector, config toggle, managed hooks, matrix usage count | Complete useful loop without a new analytics surface |
| V1.1 | Date filters and detail drilldown | Useful once enough local events accumulate |
| V2 | MCP wrapper telemetry and OpenTelemetry export | Broader observability after the local primitive is stable |

## Risks and edge cases

- Tool hook payloads differ, so attribution must be conservative.
- Multiple tools can use the same skill name; ambiguous matches must not inflate counts.
- The app may be closed while hooks run; hooks must degrade without failing the agentic workflow.
- Local event payloads may include sensitive fields; the collector stores only allowlisted normalized fields.

## Metrics or signals

- Count of traced skill invocations per skill.
- Source-tool distribution per skill.
- Last-used timestamp per skill.
- Number of unresolved events, surfaced in Config as a quality signal for later
  integration work.

## Open questions

- Should future filters default to all time or the last 30 days?
