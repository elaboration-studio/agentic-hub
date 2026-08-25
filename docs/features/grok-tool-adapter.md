# Feature: Grok Build Tool Adapter + Usage Tracing

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-08-25
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md), [DESIGN.md](../../DESIGN.md)
Related Docs: [docs/tech/modules/grok-tool-adapter.md](../tech/modules/grok-tool-adapter.md), [docs/tech/modules/local-usage-tracing.md](../tech/modules/local-usage-tracing.md), [docs/tech/reference/tool-adapter-matrix.md](../tech/reference/tool-adapter-matrix.md), [docs/features/local-skill-usage-tracing.md](./local-skill-usage-tracing.md)

## Why now

Agentic Hub already projects capabilities and traces skill usage for Codex, Claude Code, Cursor, and Kiro. Grok Build is a first-class local agentic CLI with a documented hook, skill, agent, rule, and command layout under `~/.grok/`, but the hub cannot project into it or count Grok skill invocations. Without a Grok column, usage tracing is blind to the tool the user is actually running.

## User story

As an Agentic Hub user who runs Grok Build, I want the same shared skills, agents, rules, commands, and hooks I maintain under `~/.agentic` to appear in `~/.grok/`, and I want Statistics and the Manager Usage column to count Grok skill invocations, so I can manage one library across every local agentic tool I use.

## Scope

### In scope

- Opt-in `ToolId::Grok` column (disabled by default, like Kiro / Copilot / Antigravity).
- Global projection into Grok's native home:
  - skills → `~/.grok/skills/` (symlink, nested)
  - agents → `~/.grok/agents/` (symlink, flat `*.md`)
  - rules → `~/.grok/rules/` (symlink, nested `*.md`)
  - commands → `~/.grok/commands/` (symlink, flat `*.md`)
  - hooks (default target, same as Cursor / Claude / Codex) → one Claude-style JSON file per hook id under `~/.grok/hooks/`
- Managed usage-tracer hook at `~/.grok/hooks/agentic-hub-usage-tracer-grok.json` when tracing is on and Grok is enabled.
- Attribution of Grok hook payloads: `promptId` turn identity, slash / `$skill` prompt refs, `read_file` of `SKILL.md` / agent markdown, catalog roots under `~/.grok/skills` and `<ws>/.grok/skills` plus `.agents/skills`.
- Read-only workspace inventory of `<ws>/.grok/{skills,agents,rules,hooks,commands}`.
- Config toggle, Manager column, Statistics `grok` source-tool bucket.

### Out of scope

- Grok plugins, marketplaces, personas (`.grok/personas/*.toml`), MCP, LSP, or `config.toml` / `managed_config.toml` hook layers.
- Projecting into Claude/Cursor compat paths Grok already scans (`~/.claude/skills`, `~/.cursor/skills`). Hub skills land in `~/.grok/skills/`.
- HTTP tracer hooks. Grok HTTP handlers cannot set `x-agentic-hub-token`, so the existing `usage-tracer.sh` command hook remains the emitter.
- New canonical hook events (`StopFailure`, `StopCancelled`, `SubagentStart`, `SubagentStop`, `PermissionDenied`).
- Cloud / remote Grok sessions that cannot reach the loopback collector.
- Inferring auto-invoked skills that never appear as a slash token, `$skill`, Skill-style tool, or `SKILL.md` read.

## Experience

Config → Tools gains a **Grok** accordion, off by default. Enabling it shows the Grok column in the Manager. Toggling a skill, agent, rule, command, or hook for Grok projects into the matching `~/.grok/` path. Hooks without an explicit `targets` list include Grok (same default set as Cursor / Claude / Codex). A hook that lists `targets` without `"grok"` still skips Grok. `.mdc` rules are rewritten to `.md` because Grok only loads `*.md` in `~/.grok/rules/`.

When local usage tracing is enabled and Grok is on, Agentic Hub writes a managed tracer file under `~/.grok/hooks/`. Grok loads that directory as always-trusted global hooks — no `/hooks-trust` step. Config shows Grok in the per-tool tracer diagnostics (hook installed, last event, resolved / unresolved). Disabling tracing or Grok removes only the managed tracer file; user-authored `~/.grok/hooks/*.json` stay untouched.

Statistics and the Manager Usage column treat `grok` like `codex` / `claude` / `cursor` / `kiro`. A skill invoked from Grok increments that skill's count with a Grok bucket. Same-named global and repository skills stay separate.

## Acceptance criteria

- [ ] `ToolId::Grok` appears in the Manager when enabled in Config; it is absent (or disabled) on fresh settings.
- [ ] Enabling a skill for Grok symlinks it under `~/.grok/skills/` preserving nested path.
- [ ] Enabling an agent for Grok symlinks a flat `*.md` under `~/.grok/agents/`.
- [ ] Enabling a rule for Grok symlinks it under `~/.grok/rules/`; a `.mdc` source lands as `.md`.
- [ ] Enabling a command for Grok symlinks a flat `*.md` under `~/.grok/commands/`.
- [ ] Enabling a hook for Grok (default targets or explicit `"grok"`) writes `~/.grok/hooks/<id>.json` in Claude two-level JSON with `_agenticHub` on the matcher group.
- [ ] Hooks whose `targets` omit `"grok"` do not write Grok files.
- [ ] Foreign Grok hook files and non-hub files under `~/.grok/` are never overwritten.
- [ ] Enabling tracing while Grok is on installs `agentic-hub-usage-tracer-grok.json` for `UserPromptSubmit`, `PostToolUse`, and `PostToolUseFailure`.
- [ ] Disabling tracing removes only that managed tracer file.
- [ ] A Grok `UserPromptSubmit` payload with `/grill-me` (or `$grilling`) stores one occurrence for the matching catalog skill.
- [ ] A Grok `PostToolUse` of `read_file` on a catalog `SKILL.md` stores one occurrence; `hookEventName: "post_tool_use"` is canonicalized.
- [ ] Repeated refs to the same skill in one `promptId` increment once.
- [ ] Repository skills under `<ws>/.grok/skills` resolve without a saved workspace.
- [ ] Qualified slash names (`/user:commit`, `/local:commit`) strip the scope prefix before catalog match.
- [ ] Workspace inventory lists `<ws>/.grok/{skills,agents,rules,hooks,commands}` read-only.
- [ ] No raw prompts, tool inputs, or session ids are persisted.

## Dependencies

- Existing `ToolId` / settings / adapter registry / Manager column pattern (Kiro, Copilot, Antigravity).
- Hook projection marker `_agenticHub` and `usage-tracer.sh`.
- Local usage tracing collector, catalog, and attribution pipeline.
- Grok Build hook contract (`~/.grok/docs/user-guide/10-hooks.md`): global `~/.grok/hooks/*.json`, Claude-style JSON, camelCase stdin, snake_case `hookEventName`, `promptId`, `workspaceRoot`.

## Delivery slices

| Slice | What ships | Why this cut |
| ----- | ---------- | ------------ |
| V1 | `ToolId::Grok` + settings + adapter paths + Manager/Config column | Column exists; projection is testable |
| V1.1 | Skill / agent / rule / command projection + workspace inventory | Real column, not a dead toggle |
| V1.2 | `GrokHookFile` projection + managed tracer hook | Grok actually loads the tracer |
| V1.3 | Attribution + catalog + Statistics/Config diagnostics | Full trace loop |

V1–V1.3 land together in this feature. The table is the implementation order, not separate releases.

## Risks and edge cases

- Grok stdin uses `hookEventName: "post_tool_use"` (snake_case) and `toolName: "read_file"`, not Claude's PascalCase / `Read`. Attribution must canonicalize both or Grok events look like noise.
- Grok auto-invokes skills from `description` / `when-to-use` without a slash token. Those invocations are uncounted unless a `SKILL.md` read or explicit `$skill` / `/skill` appears — same conservative contract as the other tools.
- Qualified names (`/user:commit`) must not be stored as the literal `user:commit` skill id.
- `UserPromptSubmit` is observe-only in Grok (stdout and exit code ignored). The tracer already exits 0 and must not try to block prompts.
- Matcher on `UserPromptSubmit` is ignored by Grok with a warning; keep matcher `"..."` for shared-manifest consistency.
- Grok HTTP hooks cannot carry the collector token header; never switch the tracer to `type: "http"`.
- Global Grok hooks are always trusted. Project `.grok/hooks/` is not a hub write path (workspace remains read-only).

## Metrics or signals

- Grok appears in Config tracer diagnostics when enabled.
- Per-skill usage bucket `grok` is non-zero after a traced invocation.
- Resolved vs unresolved Grok event counts in Config.

## Open questions

- None blocking V1. Whether Grok's rules directory is recursive is inferred from docs ("`*.md` files in rules directories"); nested projection matches Cursor. Flatten in a follow-up if dogfooding shows top-level-only loading.
