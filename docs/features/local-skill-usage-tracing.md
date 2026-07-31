# Feature: Local Skill Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-19
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

- Local usage tracing, enabled by default (opt-out via Config).
- Local SQLite persistence under `~/.agentic-hub/usage/trace.db`.
- Loopback-only event collection from managed tracer hooks.
- Skill usage counts joined onto global and repository-local Manager rows.
- Repository skill discovery for active Cursor, Claude Code, and Codex
  workspaces, including repositories that are not saved in Agentic Hub.
- Multi-skill attribution: every distinct skill explicitly invoked in one user
  turn is counted once.
- Agent spec usage counts joined onto existing Manager matrix rows when invoked
  via explicit slash (`/cto`), Claude `@agent-*` mention, or a single Read of an
  agent markdown file under an `/agents/` path.
- A `Usage` table column with hover details by source tool.
- Accurate-only skill attribution: exact canonical paths win, then documented
  tool scope and precedence, then unique names. Ambiguous references stay
  unresolved and do not increment a visible skill count.

### Out of scope

- Remote sync or cloud analytics.
- Inferring skill usage from free-form prompts, transcripts, or raw source code.
  Cursor, Claude, and Codex count explicit `$skill`, catalog-validated `/skill`
  (and `/agent` / `@agent-*`), Skill tool calls, `SKILL.md` links/attachments/reads,
  and agent markdown reads under an `/agents/` path from hook input. Claude slash
  commands also count via `UserPromptExpansion.command_name`. Unknown slash tokens
  that do not resolve in the local catalog are dropped (not stored as unresolved
  noise). Raw prompts and tool inputs are consumed in memory and never persisted
  or logged.
- Editing or deleting usage events from the UI.
- Non-skill capability analytics beyond storing raw terminal events.

## Experience

The Config page exposes a local usage tracing toggle. When enabled, Agentic Hub
starts a loopback collector and projects a managed tracer hook into supported,
enabled tools. Hooks forward terminal execution events to the local collector
with a short timeout (1s); on delivery failure the hook spools the payload under
`~/.agentic-hub/usage/spool/` for later drain and always exits 0 so tracing never
blocks the calling agentic tool. The collector drains the spool on startup and
every 30 seconds while tracing is enabled. Config also shows per-tool hook
installation, last-captured time, and resolved/unresolved counts so users can
distinguish collection failures from attribution gaps.

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

A **Sort by usage** control in the Manager toolbar defaults to **Latest use**
(newest first). Users can switch to **Usage count** (highest first). Rows
without usage data sort last; ties break alphabetically by name. Sorting applies
within flat kind groups and among sibling leaves in tree view; folder rows stay
grouped above leaves and remain alphabetically ordered.

On Statistics, the **Today's usage** and **Top usage** tables expose the same
two sort modes via clickable **Uses** / **Last used** column headers. **Last
used** is active by default and shows a downward arrow on the active column.

Repository-local rows retain their repository identity. A global skill and a
repository skill with the same name have separate counts. Statistics retains
the workspace label and root for historical rows even when that repository is
not currently selected in Manager.

The **Statistics** tab (beside Config) starts on **Today**, a local-calendar-day
usage table that is independent of the selected date range. Its tabs are
**Today**, **Activity** (daily activity, kind/source-tool/workspace charts),
**Top usage** (usage overview tiles followed by most-used capabilities),
**Unused** (installed-but-unused rows), and **Inventory** (total resources,
per-kind counts, enabled tools, and starred skills). Inventory remains available
when tracing is disabled. Inactive sub-tabs do not render their charts or tables.
The date range selector appears only on Activity, Top usage, and Unused; it
defaults to the last 30 days.

## Acceptance criteria

- [ ] New installs and settings files predating the `usage_tracing` block load with tracing enabled by default; settings files with an explicit `enabled: false` keep that choice.
- [ ] Enabling tracing starts the local collector and installs managed tracer hooks for supported enabled tools.
- [ ] Disabling tracing stops the collector and removes managed tracer hooks.
- [ ] A valid event containing multiple explicit skills stores one occurrence for each distinct skill.
- [ ] Repeated references to the same skill within one turn increment it once.
- [ ] Repository-local skills resolve for active Cursor, Claude Code, and Codex repositories without requiring a saved workspace.
- [ ] Same-named global and repository-local skills retain separate identities and counts.
- [ ] Overlapping prompt and terminal hook signals upgrade one occurrence instead of double-counting it.
- [ ] Legacy duplicate events with the same dedupe hash remain ignored.
- [ ] Events with missing or ambiguous skill names are stored without incrementing any skill row.
- [ ] The Manager matrix shows a `Usage` column after `Source`.
- [ ] Hovering a skill's usage count shows per-tool counts.
- [ ] Command palette copy/paste increments usage for the matching command row when tracing is enabled.
- [ ] No raw prompts, source snippets, or tool arguments are persisted.
- [ ] Config shows hook-installed state, last captured event, and resolved/unresolved diagnostics per tool.
- [ ] Config reports the collector as running only after an authenticated health probe succeeds.
- [ ] With tracing enabled, an hourly failed probe restarts the collector up to three times without reinstalling hooks.
- [ ] After three failed restarts, the user receives one desktop notification and one persistent in-app restart prompt for the outage.
- [ ] The Statistics tab shows overview metrics, charts, and tables when tracing is enabled.
- [ ] Statistics date-range filters reload dashboard aggregates without leaving the page.
- [ ] Statistics shows an enable-tracing prompt when local tracing is disabled.
- [ ] Statistics shows resource inventory (total resources, per-kind counts, enabled tools, starred skills) regardless of tracing state.
- [ ] Statistics splits content into Today/Activity/Top usage/Unused/Inventory sub-tabs; only the active tab's charts and tables render.
- [ ] Statistics opens on Today and shows a today's-usage table scoped to the local calendar day, independent of the selected date range.
- [ ] Statistics keeps resource inventory accessible in Inventory while tracing is disabled.
- [ ] Statistics top and today rows show workspace context even if the workspace is not selected.

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
| V1.3 | Statistics sub-tabs, today's-usage table, tracing on by default | Shipped: Today/Activity/Top usage/Unused/Inventory tabs, local-day usage table, opt-out default |
| V1.4 | Repository-local and multi-skill attribution | Scoped identity, tool-aware catalogs, once-per-skill-per-turn dedupe, diagnostics, and historical workspace context |
| V2 | MCP wrapper telemetry and OpenTelemetry export | Broader observability after the local primitive is stable |

## Risks and edge cases

- Tool hook payloads differ, so attribution must be conservative.
- Global and repository scopes can use the same skill name; path and tool
  precedence must disambiguate without contaminating either count.
- Hook payload paths are untrusted; canonicalization, root bounds, and symlink
  checks must complete before any repository file is read.
- The app may be closed while hooks run; hooks must degrade without failing the agentic workflow.
- The checker runs only while the Agentic Hub process is alive; a fully quit app cannot monitor its collector.
- A healthy listener does not prove that a later SQLite write or skill attribution succeeds; those remain separate diagnostics.
- Local event payloads may include sensitive fields; the collector stores only allowlisted normalized fields.

## Metrics or signals

- Count of traced skill invocations per skill.
- Source-tool distribution per skill.
- Last-used timestamp per skill.
- Resolved and unresolved event counts per tool, surfaced in Config as an
  attribution quality signal.

## Open questions

- Cloud-agent telemetry remains intentionally out of scope because cloud agents
  cannot reach the loopback collector.
