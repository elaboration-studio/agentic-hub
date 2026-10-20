# Feature: Agents and run bundles

Status: In Progress
Mode: Detailed
Owner: Arno
Last Updated: 2026-10-10
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [suite-presets.md](./suite-presets.md)
Related Docs: [docs/tech/modules/agent-bundles.md](../tech/modules/agent-bundles.md), [docs/tech/reference/ehub-cli-contract.md](../tech/reference/ehub-cli-contract.md)
Essential Companion: [agent-bundles.essential.md](./agent-bundles.essential.md)

## Purpose

A suite already says "these skills, rules, hooks, and subagents belong together". This feature lets a suite become an **agent** — with a face (emoji), its own instructions, the MCP servers and CLIs it needs — and lets another local program run that agent in any supported CLI without changing the user's global setup. eCanvas is the first such program: it assigns Tasks to "Arno's CTO" instead of "MacBook Pro – Cursor".

## User goal

> I keep a CTO suite and a CPO suite. I want each to be a worker I can hand a task to. When both run in Cursor on my Mac at once, neither should see the other's skills, and my everyday Cursor setup should stay mine.

## Scope

1. **Agent block on a suite.** Suite Manager gains an Agent section: emoji, instructions (markdown), required CLIs (picked from the CLI catalog). Any suite can be used as an agent; the block is optional.
2. **MCP as a capability kind.** `~/.agentic/mcp/<id>/mcp.json` declares one server (stdio command or HTTP url, env var names only). Suites can include it. v1 never writes MCP into a tool's global config.
3. **Version.** Each suite has a content version (12 hex chars) that changes when anything it contains changes.
4. **Run bundle.** For one suite version and one harness (Cursor, Claude, Codex, Grok), eHub writes an immutable bundle under `~/.agentic-hub/bundles/`. Cursor and Claude get a plugin directory; Codex and Grok get instructions plus a skills folder for the prompt.
5. **Headless `ehub` CLI.** `ehub agents list|show`, `ehub bundle`, `ehub version`, JSON only. It works with the app closed. The app keeps `~/.agentic-hub/bin/ehub` linked to itself.

## Acceptance criteria

- [ ] A suite with an agent block round-trips through the suites file; a suite without one is unchanged on disk.
- [ ] An `mcp/<id>/mcp.json` source appears as an MCP capability, can be added to a suite, and is never projected into a tool home.
- [ ] `ehub agents list` prints every suite with name, emoji, version, and capability counts.
- [ ] Editing a rule or skill file in a suite changes its version; editing a suite timestamp does not.
- [ ] `ehub bundle <id> --harness cursor` prints a manifest whose `pluginDir` contains the suite's skills, subagents, commands, and hooks, and whose `instructionsFile` holds the agent instructions followed by the suite's rules.
- [ ] Running it twice reuses the same bundle. Two concurrent builds both succeed and leave one bundle.
- [ ] Bundles older than 7 days that are not the current version are deleted on the next build.
- [ ] The app creates `~/.agentic-hub/bin/ehub` on launch and never replaces a real file there.
- [ ] Global suite apply, bindings, and the watcher behave exactly as before.

## Out of scope

- Writing MCP servers into each tool's global config (later, via the same kind).
- Installing missing CLIs (eHub reports them; the existing CLI catalog already shows install hints).
- Multi-machine sync and cloud profiles. A later cloud harness can consume the same bundle contract; no sync lives here.
- Any network call. The CLI never talks to eCanvas; eCanvas calls it.

## Edge cases

| Case | Behavior |
| --- | --- |
| Two agents run in one CLI at once | Separate bundles; nothing shared is written |
| Suite edited mid-run | New version, new bundle; the running one is untouched |
| Suite deleted | `ehub bundle` exits 3 `NOT_FOUND`; the caller fails the run |
| Skill basename collision | First by id wins; the other is listed in `skipped` |
| Required CLI missing | Reported `installed: false`; the caller decides |
| Real file at `~/.agentic-hub/bin/ehub` | Left alone; logged |
