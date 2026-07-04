# Feature: Global Installed Resources

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-04
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [docs/features/workspace-inventory.md](./workspace-inventory.md)
Related Docs: [docs/tech/modules/workspace-inventory.md](../tech/modules/workspace-inventory.md), [docs/tech/reference/tool-adapter-matrix.md](../tech/reference/tool-adapter-matrix.md)

## Why now

The Manager's Global view currently shows resources from configured Agentic Hub
source roots and whether the Hub has projected them into each tool. Users also
install skills, agents, rules, hooks, and commands directly into tool-native
global folders such as `~/.codex`, `~/.claude`, `~/.cursor`, `~/.kiro`,
`~/.copilot`, and `~/.gemini/config`. Those resources affect the tools but are
invisible in All Resources unless the user also registers them as Hub sources.

This feature makes the Global view a fuller audit of what the user's tools can
see, without claiming ownership of files the Hub did not create.

## What it does

- **Shows unmanaged global installs.** The Global Manager merges configured
  source-root resources with read-only rows discovered from enabled tools'
  native global folders.
- **Labels by tool source.** Unmanaged rows use source labels such as `Codex`,
  `Claude`, `Cursor`, `Kiro`, `Copilot`, and `Antigravity`, so the existing
  Source filter narrows to one tool's installed resources.
- **Keeps managed rows editable.** Hub source-root rows keep their existing
  toggles and Apply flow.
- **Keeps unmanaged rows read-only.** Unmanaged rows render static present
  checks, support search / source / type filters, and expose Open / Reveal
  actions. They cannot be enabled, disabled, deleted, imported, or synced in v1.
- **Avoids duplicate rows.** If an installed file is already represented by a
  Hub-managed projection state, the unmanaged scanner suppresses that duplicate
  row.

## Scope

- Scope: Global Manager only.
- Tools: configured enabled tools only.
- Kinds: skills, agents, rules, commands, plus one-file-per-hook layouts for
  Kiro and Copilot.
- Out of scope: importing unmanaged resources into a Hub source, deleting tool
  files, disabling unmanaged resources, decomposing aggregate hook files, and
  parsing Hub-managed markdown blocks into individual rule rows.

## Acceptance criteria

- In Global scope, All Resources includes unmanaged resources from enabled tool
  folders even when they are not under any configured Agentic Hub source.
- The Source filter includes one option per discovered unmanaged tool source.
- Unmanaged rows are visibly present for their owning tool and cannot be toggled
  or included in Apply / rule sync / hook sync calls.
- Open original and Reveal in Finder work for unmanaged rows through the same
  server-side path allowlist as managed rows.
- Existing workspace inventory behavior is unchanged.
