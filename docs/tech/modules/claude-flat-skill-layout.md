# Module: Claude Flat Skill Layout

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../../features/mvp-unified-agentic-capability-manager.md), [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Why this exists

Claude Code's **skill** discovery scans only the top level of `~/.claude/skills/`. It does **not** recurse into subdirectories ([docs](https://code.claude.com/docs/en/skills); issues [#18192](https://github.com/anthropics/claude-code/issues/18192) / [#10238](https://github.com/anthropics/claude-code/issues/10238)). A folder like `~/.claude/skills/dev/` is treated as one opaque skill named `dev`, so any skill nested below becomes invisible to Claude Code.

Claude **agents** are different: the agent loader scans `~/.claude/agents/` **recursively**, and a subagent's identity comes from its `name` frontmatter, not its path ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)). So agents keep their nesting like every other tool — only skills need flattening.

Codex, Cursor, and OpenClaw all recurse for both kinds, so they tolerate the nested category layout that the shared root (`~/.agentic/skills/dev/repo-research/SKILL.md`) uses by convention.

Agentic Hub's projection layer reconciles these two contracts deterministically.

## Contract

Each tool adapter declares a per-kind **layout strategy**:

| Tool | `skill_layout` | `agent_layout` |
|------|----------------|----------------|
| Claude Code | `Flat` | `Nested` |
| Codex | `Nested` | `Nested` |
| Cursor | `Nested` | `Nested` |
| OpenClaw | `Nested` | `Nested` |

`Flat` collapses `item.relative_path` to its basename when computing the target path. `Nested` preserves `item.relative_path` verbatim. Only `(Claude, Skill)` is `Flat`.

| Source (`~/.agentic/skills/...`) | Codex / Cursor target | Claude target |
|----------------------------------|------------------------|---------------|
| `dev/repo-research/SKILL.md` | `<skillsPath>/dev/repo-research/` | `<skillsPath>/repo-research/` |
| `arno/cto/code-review/SKILL.md` | `<skillsPath>/arno/cto/code-review/` | `<skillsPath>/code-review/` |
| `agents/group/foo.md` (agent) | `<agentsPath>/group/foo.md` | `<agentsPath>/group/foo.md` |

Rules are not affected — rules always use their `relativePath` verbatim regardless of tool, because Claude reads rules from a single instruction file (`CLAUDE.md`) via `markdown_section_sync`, not from the filesystem.

## Where the decision lives

- Type: `Layout` enum in `agentic-core::types::Layout` (mirrored to TS via `ts-rs`)
- Defaults: `AdapterRegistry::default_skill_layout()` / `default_agent_layout()` per tool id
- Application: `ResolvedAdapter::target_path_for(item)` is the single call site

The layout is a **tool-intrinsic property**, not a user setting. It reflects what each tool's runtime actually scans. Exposing it as a configurable setting would invite users to break their own Claude install.

## Basename collisions

`Flat` layout introduces a real risk: two source items with the same basename project to the same target path. Example:

- `~/.agentic/skills/dev/repo-research/`
- `~/.agentic/skills/marketing/repo-research/`

Both would target `~/.claude/skills/repo-research`. Without a guard, enabling the second silently replaces the first link, and the user loses one capability without warning.

`planner::compute_flat_layout_collisions` resolves this:

1. Group all planned ops by `(tool, target_path)` where `tool.layout == Flat`.
2. For each group with > 1 entry, sort by `item.id` (lexicographic, deterministic).
3. The first item wins — its `CreateLink` / `ReplaceLink` op is preserved.
4. Every later collision becomes a `SkipConflict` operation with a reason like:

   > Basename collision in flat layout: `marketing/repo-research` would overwrite `dev/repo-research` at `~/.claude/skills/repo-research`. Rename the source item to disambiguate.

The user-facing fix is to rename a source item under `~/.agentic/`, not to invent ad-hoc Claude-only nesting.

## Inspect-time behavior

When the manager inspects current disk state, both colliding items resolve to the same `target_path`. Whichever source the existing symlink points at sees `state: Enabled`; the other sees `state: ForeignLink`. This surfaces the collision in the inventory before the user attempts to apply.

## Apply safety

- Apply still refuses to remove a real (non-symlink) directory or overwrite a real file. The flat layout does not relax those guards.
- For `ReplaceLink`, the apply removes only a symlink (or a managed copy file with manifest attribution) before recreating it. Real files at the target path always block.
- The collision guard only fires when the user enables both items in the same plan. A pre-existing on-disk collision created outside Agentic Hub still appears as `ForeignLink` and the user must resolve it explicitly.

## Tests

`crates/agentic-core/src/planner/tests.rs` covers:

- Claude flattens `dev/repo-research` → `repo-research` for skills; Claude **agents** stay nested (`team/reviewer.md` → `agents/team/reviewer.md`, asserted in `adapter_registry.rs`)
- Codex and Cursor preserve nested paths
- `planner::build_plan` emits exactly one `CreateLink` and one `SkipConflict` when two items collide on the same flat target
- Inspect-time: colliding items show one `Enabled` and one `ForeignLink`
- Removing both items reduces to one `RemoveLink` plus a no-op for the loser

## Future extensions

- If Claude Code adds recursive scanning, switch the default in `AdapterRegistry::default_skill_layout(Claude)` from `Flat` to `Nested`. No other code change required.
- If a future tool needs `Flat` for one kind and `Nested` for the other, the existing per-kind fields (`skill_layout`, `agent_layout`) already model that.

## Open questions

- Should we offer a "preview the flattened paths" affordance in the UI so users see ahead of time which Claude targets they will produce? Defer to P2; the collision detection already surfaces problems at plan time.
- Should the collision-pass tie-breaker be user-configurable (e.g. prefer `dev/` over `marketing/`)? Decision: no in v1; lexicographic ordering keeps behavior deterministic and easy to reason about.
