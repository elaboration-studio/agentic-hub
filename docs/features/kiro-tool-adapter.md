# Feature: Kiro Tool Adapter

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-29
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/kiro-tool-adapter.md](../tech/modules/kiro-tool-adapter.md), [docs/tech/reference/tool-adapter-matrix.md](../tech/reference/tool-adapter-matrix.md)

## Purpose

Project shared agentic capabilities — skills, agents, rules (as steering files), and hooks — into [Amazon Kiro](https://kiro.dev/)'s native `~/.kiro/` layout from the Agentic Hub manager. Kiro is **disabled by default**; enable it in Config when you use the IDE or CLI.

## User goal

> I use Kiro alongside Cursor and Claude. I want the same shared skills, custom agents, steering rules, and shell hooks I maintain under `~/.agentic` to appear in `~/.kiro/` without hand-copying, and I want workspace inventory to show what a project's `.kiro/` tree already contains.

## Acceptance criteria

- [ ] `ToolId::Kiro` appears in the manager matrix when enabled in Config
- [ ] Skills symlink into `~/.kiro/skills/` (nested, Agent Skills standard)
- [ ] Agents symlink into `~/.kiro/agents/` as flat `*.md` (IDE format)
- [ ] Rules sync into the managed block in `~/.kiro/steering/AGENTS.md` (always included; not per-file steering)
- [ ] Hooks with `"targets": ["kiro"]` project to `~/.kiro/hooks/<id>.json` in Kiro v1 schema
- [ ] Workspace scope scans `<ws>/.kiro/{skills,agents,steering,hooks}` read-only
- [ ] Foreign Kiro hook files and steering files are never overwritten
- [ ] Commands are unsupported (no projection target in v1)

## Limitations (v1)

- Kiro CLI reads `.json` agent configs in the same directory as IDE `.md` agents ([issue #8040](https://github.com/kirodotdev/Kiro/issues/8040)); the hub projects markdown only
- Hook projection supports shell `command` actions only (not Kiro "Ask Kiro" agent-prompt actions)
- Kiro-only hook triggers (`PreTaskExec`, `PostTaskExec`, file create/delete) are not sourced from hub hooks
- `.mdc` rules are symlinked as-is; verify Kiro loads them or rename to `.md` in a follow-up

## Out of scope

- Powers, MCP config, specs, prompts directory
- Automatic `"targets"` default inclusion (Kiro is opt-in like OpenStandard hooks)
