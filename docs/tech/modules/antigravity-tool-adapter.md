---
name: Google Antigravity tool adapter module
slug: antigravity-tool-adapter
---

# Module: Google Antigravity Tool Adapter

`ToolId::Antigravity` is disabled by default. Skills use managed copy with flat
layout under `~/.gemini/config/skills` (Antigravity ignores symlinks and scans
only the top level); rules use `MarkdownSectionSync` in `~/.gemini/AGENTS.md`;
hooks use the shared `JsonSection` projection at `~/.gemini/config/hooks.json`.
Agents and commands have no projection mode.

Hooks are opt-in through `"targets": ["antigravity"]` and retain the common
managed marker and foreign-entry-preserving behavior from `hook_sync`.

Legacy configs with the pre-0.10.1 default `~/.gemini/skills` are rewritten once
to `~/.gemini/config/skills` via `Settings::migrate_antigravity_skills_path()`.

Workspace inventory reads skills and rules from `.agents` plus the legacy
`.agent` paths. It does not decompose aggregate `.agents/hooks.json` content;
that is consistent with the existing aggregate hook-file boundary for Codex,
Claude, and Cursor.
