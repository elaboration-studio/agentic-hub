---
name: GitHub Copilot tool adapter
slug: copilot-tool-adapter
---

# Feature: GitHub Copilot Tool Adapter

GitHub Copilot is an opt-in tool column. When enabled, Agentic Hub projects
shared skills, agents, rules, and hooks into Copilot's global directories.
Hooks target Copilot only when their manifest explicitly lists `"copilot"`.

Global projection uses `~/.copilot/skills`, `~/.copilot/agents`,
`~/.copilot/instructions`, and one JSON file per hook under
`~/.copilot/hooks`. Agent and rule filenames are rendered with Copilot's
`.agent.md` and `.instructions.md` suffixes.

Workspace scope remains read-only and inventories `.github/skills`,
`.github/agents`, `.github/instructions`, `.github/hooks`, and the shared
`.agents/skills` directory.
