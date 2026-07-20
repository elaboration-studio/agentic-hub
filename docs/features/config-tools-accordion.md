# Feature: Config Tools Accordion

Status: In Progress
Mode: Essential
Owner: Arno
Last Updated: 2026-07-20
Depends On: [DESIGN.md](../../DESIGN.md), [PRODUCT.md](../../PRODUCT.md)

## Why now

The Config → Tools panel lists every tool with its full projection path table
always expanded. That section dominates the Config scroll and makes enable/disable
harder to scan. Folding path details behind a classic accordion reclaims vertical
space without changing any settings contract.

## User story

As a user on Config, I want each tool’s projection paths collapsed by default and
only one tool expanded at a time, so I can scan enable toggles quickly and open
paths only when I need them.

## Core scope

- Config → Tools uses a single-select accordion (`type="single"`, `collapsible`).
- All tools start collapsed; opening one closes any other.
- Header row keeps the enable `Switch` + tool name; only the Skills / Agents /
  Rules / Hooks / Commands path list folds.
- Chevron affordance: `ChevronRight` (collapsed) / `ChevronDown` (expanded).
- No persistence of open state; session-local only.

## Acceptance signal

- Tools panel shows one compact row per tool by default.
- Expanding a tool reveals its path table; expanding another collapses the first.
- Toggling a tool’s enable Switch never expands or collapses that accordion item.

## Risks

- Switch inside/near AccordionTrigger must not nest interactive controls incorrectly
  or steal expand clicks — keep Switch outside the trigger.

## Open questions

- None for V1.
