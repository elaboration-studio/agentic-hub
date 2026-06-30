---
name: Google Antigravity tool adapter
slug: antigravity-tool-adapter
---

# Feature: Google Antigravity Tool Adapter

Google Antigravity is an opt-in tool column. When enabled, Agentic Hub projects
shared skills to `~/.gemini/config/skills` (managed copy, flat layout), rules
into the managed block in `~/.gemini/AGENTS.md`, and explicitly targeted hooks
into `~/.gemini/config/hooks.json`. Agents and commands are unsupported.

Workspace scope remains read-only and inventories skills and rules from both
the current `.agents` layout and the legacy `.agent` layout. Aggregate
`.agents/hooks.json` content is not decomposed into individual inventory rows.
