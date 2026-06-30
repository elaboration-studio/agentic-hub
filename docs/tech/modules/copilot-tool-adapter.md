---
name: GitHub Copilot tool adapter module
slug: copilot-tool-adapter
---

# Module: GitHub Copilot Tool Adapter

`ToolId::Copilot` is disabled by default. Its adapter uses flat skill
symlinks, flat agent symlinks renamed to `*.agent.md`, nested rule symlinks
renamed to `*.instructions.md`, and `CopilotHookFile` projection.

`copilot_hook_sync.rs` writes one `version: 1` JSON file per hook id under
`hooksDir`. Event keys are camelCase. Every managed entry carries
`_agenticHub.hookId`, `sourceHash`, and marker version. Apply may create a
missing file or replace a fully hub-managed file; it refuses directories,
foreign entries, empty unmarked files, and malformed JSON. Removal deletes only
files whose entries are fully hub-managed.

Legacy settings with no `hooksDir` resolve to `~/.copilot/hooks`; users disable
hook projection with `hooksEnabled`, not by clearing the path.

Workspace inventory reads `.github/skills`, `.github/agents/*.agent.md`,
`.github/instructions/*.instructions.md`, `.github/hooks/*.json`, and
`.github/copilot-instructions.md` without writing to the project.
