# Module: Claude Flat Skill Layout

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-15
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../../features/mvp-unified-agentic-capability-manager.md), [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Why this exists

Claude Code's **skill** discovery scans only the top level of `~/.claude/skills/`. It does **not** recurse into subdirectories ([docs](https://code.claude.com/docs/en/skills); issues [#18192](https://github.com/anthropics/claude-code/issues/18192) / [#10238](https://github.com/anthropics/claude-code/issues/10238)). A folder like `~/.claude/skills/dev/` is treated as one opaque skill named `dev`, so any skill nested below becomes invisible to Claude Code.

The same non-recursive constraint applies to **subagent** loaders in two other tools: Cursor scans only the top level of `~/.cursor/agents/` (identity is the filename; [Cursor subagents](https://cursor.com/docs/subagents)), and Codex loads only top-level `*.toml` from `~/.codex/agents/` ([Codex subagents](https://developers.openai.com/codex/subagents)). So Cursor and Codex **agents** flatten too.

Claude **agents** are the exception: the agent loader scans `~/.claude/agents/` **recursively**, and a subagent's identity comes from its `name` frontmatter, not its path ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)). So Claude agents keep their nesting — flattening them would collide same-`name` agents from different folders.

The rule is therefore not "Claude is special" but "**flatten wherever the loader is non-recursive**": Claude skills, Cursor agents, Codex agents. Everything else (including Claude agents, and all skills/commands on the recursive tools) stays nested. Agentic Hub's projection layer reconciles these contracts deterministically.

## Contract

Each tool adapter declares a per-kind **layout strategy**:

| Tool | `skill` layout | `agent` layout |
|------|----------------|----------------|
| Claude Code | `Flat` | `Nested` |
| Codex | `Nested` | `Flat` |
| Cursor | `Nested` | `Flat` |
| OpenClaw | `Nested` | `Nested` |
| OpenStandard | `Nested` | `Nested` |

`Flat` collapses `item.relative_path` to its basename when computing the target path. `Nested` preserves `item.relative_path` verbatim. `Flat` applies to `(Claude, Skill)`, `(Cursor, Agent)`, and `(Codex, Agent)`.

| Source | Nested-tool target | Flat-tool target |
|--------|--------------------|------------------|
| `dev/repo-research/SKILL.md` (skill) | Codex/Cursor `<skillsPath>/dev/repo-research/` | Claude `<skillsPath>/repo-research/` |
| `group/foo.md` (agent) | Claude `<agentsPath>/group/foo.md` | Cursor `<agentsPath>/foo.md`, Codex `<agentsPath>/foo.toml` |

Rules are not affected — rules always use their `relativePath` verbatim regardless of tool, because Claude reads rules from a single instruction file (`CLAUDE.md`) via `markdown_section_sync`, not from the filesystem.

## Where the decision lives

- Type: `Layout` enum in `agentic-core::adapter_registry::Layout`
- Decision: `ResolvedAdapter::layout_for(kind)` is the single per-`(tool, kind)` match
- Application: `ResolvedAdapter::target_path_for(item)` is the single call site

The layout is a **tool-intrinsic property**, not a user setting. It reflects what each tool's runtime actually scans. Exposing it as a configurable setting would invite users to break their own Claude install.

## Basename collisions

`Flat` layout introduces a real risk: two source items with the same basename project to the same target path. Example:

- `~/.agentic/skills/dev/repo-research/`
- `~/.agentic/skills/marketing/repo-research/`

Both would target `~/.claude/skills/repo-research`. Without a guard, enabling the second silently replaces the first link, and the user loses one capability without warning.

`planner::resolve_target_collisions` resolves this (the same pass also catches cross-source clashes, not just flat-basename ones):

1. Group all writing ops by `target_path`.
2. For each group with > 1 entry, sort by `item_id` (lexicographic, deterministic).
3. The first item wins — its create/replace op is preserved.
4. Every later collision becomes a `SkipConflict` op whose reason names the winner (e.g. `Target path already taken by 'skill:dev/repo-research'`).

The user-facing fix is to rename a source item under `~/.agentic/`, not to invent ad-hoc per-tool nesting.

The same flat target is also reached when an agent moves from nested to flat across an upgrade. There the previous nested managed copy is not a *plan-time* collision (no two items want the same path); instead the applier prunes it at write time via `managed_copy::prune_other_paths_for_item` — any copy of the **same item** at a different path is deleted (with its now-empty folder) when the new flat copy is written. Only hub-managed copies are touched.

## Inspect-time behavior

When the manager inspects current disk state, both colliding items resolve to the same `target_path`. Whichever source the existing symlink points at sees `state: Enabled`; the other sees `state: ForeignLink`. This surfaces the collision in the inventory before the user attempts to apply.

## Apply safety

- Apply still refuses to remove a real (non-symlink) directory or overwrite a real file. The flat layout does not relax those guards.
- For `ReplaceLink`, the apply removes only a symlink (or a managed copy file with manifest attribution) before recreating it. Real files at the target path always block.
- The collision guard only fires when the user enables both items in the same plan. A pre-existing on-disk collision created outside Agentic Hub still appears as `ForeignLink` and the user must resolve it explicitly.

## Tests

`crates/agentic-core/src/planner/tests.rs` covers:

- Claude flattens `dev/repo-research` → `repo-research` for skills; Claude **agents** stay nested (`team/reviewer.md` → `agents/team/reviewer.md`, asserted in `adapter_registry.rs`)
- Cursor/Codex flatten agents (`zoom/cto.md` → `cto.md` / `cto.toml`, asserted in `adapter_registry.rs`); the applier prunes a superseded nested copy of the same item (`applier.rs`, `managed_copy.rs`)
- Codex and Cursor preserve nested paths for skills/commands
- `planner::build_plan` emits exactly one `CreateLink` and one `SkipConflict` when two items collide on the same flat target
- Inspect-time: colliding items show one `Enabled` and one `ForeignLink`
- Removing both items reduces to one `RemoveLink` plus a no-op for the loser

## Future extensions

- If a tool adds recursive scanning for a kind, flip its arm in `ResolvedAdapter::layout_for` from `Flat` to `Nested`. No other code change required.
- The layout is decided per `(tool, kind)` in the single `layout_for` match, so any future flat/nested split is a one-line addition.

## Open questions

- Should we offer a "preview the flattened paths" affordance in the UI so users see ahead of time which Claude targets they will produce? Defer to P2; the collision detection already surfaces problems at plan time.
- Should the collision-pass tie-breaker be user-configurable (e.g. prefer `dev/` over `marketing/`)? Decision: no in v1; lexicographic ordering keeps behavior deterministic and easy to reason about.
