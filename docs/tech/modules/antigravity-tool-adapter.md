---
name: Google Antigravity tool adapter module
slug: antigravity-tool-adapter
---

# Module: Google Antigravity Tool Adapter

`ToolId::Antigravity` is disabled by default. Skills use nested symlinks under
`~/.gemini/skills`; rules use `MarkdownSectionSync` in
`~/.gemini/AGENTS.md`; hooks use the shared `JsonSection` projection at
`~/.gemini/config/hooks.json`. Agents and commands have no projection mode.

Hooks are opt-in through `"targets": ["antigravity"]` and retain the common
managed marker and foreign-entry-preserving behavior from `hook_sync`.

Workspace inventory reads skills and rules from `.agents` plus the legacy
`.agent` paths. It does not decompose aggregate `.agents/hooks.json` content;
that is consistent with the existing aggregate hook-file boundary for Codex,
Claude, and Cursor.
