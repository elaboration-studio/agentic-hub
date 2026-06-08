# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.7.0] — 2026-06-08

### Highlights

- **Commands — a fifth capability kind.** Keep your slash-command prompts (the
  Cursor / Claude Code / Codex kind) in one place: `~/.agentic/commands/`, in
  whatever folders you like. Enable them per tool from the Hub and they project
  into each tool's commands directory — symlinked for Cursor (`~/.cursor/commands`),
  Codex (`~/.codex/prompts`), and OpenStandard (`~/.agents/commands`), and copied
  for Claude (`~/.claude/commands`, whose loader doesn't follow symlinks). They
  show up as a "Commands" column in the manager and in the read-only workspace
  inventory.
- **Copy or edit from the palette.** Summon the palette, find a command, and press
  **Enter to copy its body to the clipboard** — paste it anywhere, even into a tool
  that has no command concept. **Alt+Enter opens the source file** for editing.
- **A starter command in the demo.** First-run scaffold now seeds a couple of
  nested example commands so there's something to try immediately.

### Changes

- Added the `command` capability kind and per-tool `commandsPath`. OpenClaw has no
  command concept and is unsupported for commands.
- New `cmd_read_capability_body` IPC (allowlist-gated) and the
  `tauri-plugin-clipboard-manager` plugin power the palette copy action.

### Migration

- **None required.** Existing configs load unchanged — the new `commandsPath`
  field is injected with the right per-tool default, so commands project without
  re-saving settings.

### Known Issues

- None.
