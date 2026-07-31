# Agentic Hub 0.16.0 — Experience and Recovery

Status: Shipped
Mode: Essential
Owner: Agentic Hub
Last Updated: 2026-07-31
Depends On: [PRODUCT.md](../../PRODUCT.md), [DESIGN.md](../../DESIGN.md), [ARCHITECTURE.md](../../ARCHITECTURE.md)
Related Docs: [suite-presets.md](suite-presets.md), [command-palette.md](command-palette.md), [local-skill-usage-tracing.md](local-skill-usage-tracing.md), [suite-bindings.md](../tech/modules/suite-bindings.md), [cli-tools.md](../tech/modules/cli-tools.md)

## Why now

Agentic Hub already centralizes projection, suites, discovery, and usage insight, but its busiest surfaces still make users translate internal state into a recovery path. Version 0.16.0 turns those surfaces into one coherent control experience: stale managed copies explain how to recover, suites share the Manager's table language, today's usage leads Statistics, frequent resources come first, and direct shortcuts remove the extra palette step.

## User story

As a power user managing capabilities across tools, I want the Hub to prioritize my current work and guide me through drift safely, so that I can recover, switch contexts, and find resources without leaving the product or risking user-owned files.

## Core scope

- Guide every stale managed copy through staged source refresh, live suite-binding re-sync, or manual source/target inspection while preserving plan-then-apply safety.
- Merge Suites into the Manager rail and reuse the Manager table frame for suite creation, editing, stale-reference cleanup, and apply.
- Make Statistics Today-first, move range-bound summaries into Top usage, and keep Inventory available when tracing is disabled.
- Order Resources as Skills, Tools, Sessions; add ripgrep to the data-only bundled CLI catalog.
- Add validated, transactional global shortcuts for the hub, all resources, skills, and commands while retaining in-palette shortcuts.
- Preserve `#/suites` and Open Suites as compatibility aliases into Manager Suites mode.

## Acceptance signal

- A user can recover unowned, selected-suite, and base-suite stale projections without force, deletion, or binding changes.
- Manager Global, Suite, and Workspace scopes use one table vocabulary without changing existing Global or Workspace behavior.
- Statistics opens on the local calendar day's usage; Inventory works independently of tracing; range selection appears only where it applies.
- Resources default to Skills only when skills.sh is enabled and otherwise fall back to Tools; ripgrep reports through the existing fixed `program + args` probe.
- All four shortcut fields reject malformed or duplicate accelerators, replace registrations atomically, and launch the requested palette mode even when the palette is already visible.
- Generated Rust/TypeScript contracts, focused regression tests, full suites, build, formatting, lint, and manual Tauri smoke checks pass.

## Risks

- Suite UI reuse must not couple editable suite inclusion state to Manager projection state; the suite store remains the source of truth for suite IDs and drafts.
- Shortcut replacement crosses an OS boundary, so rollback must restore the prior complete working set before settings persistence can succeed.
- Stale recovery must remain limited to `stale`; broken and foreign states keep their existing explicit conflict handling.
- The WebView remains untrusted. Shortcut strings are parsed as accelerators, bundled CLI checks remain trusted catalog data, and every filesystem mutation continues through typed IPC and the existing safe planner/applier pipeline.

## Open questions

None. The implementation and release decisions are approved for 0.16.0.
