# Agents and run bundles — essential

Status: In Progress
Mode: Essential
Owner: Arno
Last Updated: 2026-10-10
Detailed Source: [agent-bundles.md](./agent-bundles.md)

**One line:** a suite can be an agent, and eHub can hand any local program a sealed, versioned bundle of that agent for one CLI run.

## Why

Global projection makes one CLI one agent. Real work needs several agents sharing one Mac and one CLI at once — a CTO and a CPO both in Cursor — without stepping on each other or on the user's everyday setup.

## Shape

- **Agent = suite + optional agent block** (emoji, instructions, required CLIs). MCP servers join suites as a new capability kind.
- **Version** = hash of everything the suite contains.
- **Bundle** = immutable copy under `~/.agentic-hub/bundles/<suite>/<version>/<harness>/`: a plugin dir for Cursor and Claude, instructions plus skills for Codex and Grok.
- **`ehub` CLI** = the app binary in headless mode. `agents list`, `bundle <id> --harness <h>`, JSON only.

## Decisions

- Bundles are copied and published with one atomic rename, so concurrent builds are safe without locks.
- Rules fold into instructions for every harness, so a rule means the same thing everywhere.
- The base suite stays global; agents mount on top.
- MCP is bundle-only in v1. No global MCP writes yet.
