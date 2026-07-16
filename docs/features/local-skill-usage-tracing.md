# Feature: Local Skill Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-17
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

The collector status is an authenticated loopback health probe, not a cached
process flag. While tracing is enabled, Agentic Hub checks it hourly. A failed
check restarts the collector and probes it again up to three times, with a
five-second gap between attempts. If every attempt fails, the app sends one
desktop notification and a persistent in-app warning for that outage; both tell
the user that restarting Agentic Hub is the next step. The Config page also
shows an inline warning whenever an active status check finds tracing paused.

The Manager matrix adds a `Usage` column after `Source`. Skill, agent, and
command rows show the total attributed execution count. Hovering the number
shows per-tool counts (or `Palette` for command palette usage) and the last-used
timestamp. Rule and hook rows show `-` in v1.

The **Statistics** tab (beside Config) shows a **resource inventory** section
(always visible) with total resources, per-kind counts, enabled tools, and
starred skills. When local usage tracing is enabled, dashboard cards and
recharts charts over the same local trace database show daily activity, usage by
kind and source tool, workspace breakdown, top-used capabilities, and
installed-but-unused rows. Date range filters default to the last 30 days.

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
- [ ] Config reports the collector as running only after an authenticated health probe succeeds.
- [ ] With tracing enabled, an hourly failed probe restarts the collector up to three times without reinstalling hooks.
- [ ] After three failed restarts, the user receives one desktop notification and one persistent in-app restart prompt for the outage.
- [ ] The Statistics tab shows overview metrics, charts, and tables when tracing is enabled.
- [ ] Statistics date-range filters reload dashboard aggregates without leaving the page.
- [ ] Statistics shows an enable-tracing prompt when local tracing is disabled.
- [ ] Statistics shows resource inventory (total resources, per-kind counts, enabled tools, starred skills) regardless of tracing state.

## Dependencies

- SQLite storage in `agentic-core`.
- Tauri collector and IPC commands in `agentic-hub`.
- Existing hook projection conventions and `_agenticHub` marker semantics.
- Existing generated Rust-to-TS type pipeline.

## Delivery slices

| Slice | What ships | Why this cut |
| ----- | ---------- | ------------ |
| V1 | Local store, collector, config toggle, managed hooks, matrix usage count | Complete useful loop without a new analytics surface |
| V1.1 | Statistics page with date filters, charts, and drilldown tables | Shipped: overview, recharts, top-used, unused-installed |
| V1.2 | Collector health checks and recovery | Shipped: authenticated probe, hourly recovery, and restart guidance |
| V2 | MCP wrapper telemetry and OpenTelemetry export | Broader observability after the local primitive is stable |

## Risks and edge cases

- Tool hook payloads differ, so attribution must be conservative.
- Multiple tools can use the same skill name; ambiguous matches must not inflate counts.
- The app may be closed while hooks run; hooks must degrade without failing the agentic workflow.
- The checker runs only while the Agentic Hub process is alive; a fully quit app cannot monitor its collector.
- A healthy listener does not prove that a later SQLite write or skill attribution succeeds; those remain separate diagnostics.
- Local event payloads may include sensitive fields; the collector stores only allowlisted normalized fields.

## Metrics or signals

- Count of traced skill invocations per skill.
- Source-tool distribution per skill.
- Last-used timestamp per skill.
- Number of unresolved events, surfaced in Config as a quality signal for later
  integration work.

## Open questions

- Should future filters default to all time or the last 30 days?
