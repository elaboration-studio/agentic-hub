# Feature: Open Original & Projected Files

Status: Implemented
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-01
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.permissions.md](../../ARCHITECTURE.permissions.md)
Related Docs: [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md)

## Why now

The manager shows what is enabled per tool, but to inspect or edit a capability
the user had to leave the app and hunt for the file on disk. Worse, a tool
actually references a *projected* file (a symlink, a managed copy, or a managed
block inside an instruction file), and finding that real location by hand is
tedious. This feature lets the user jump straight from a row to either the
original source file or the file a tool actually uses.

## User story

As a user scanning the capability matrix, I want a per-row menu to open the
original file in my preferred editor and to open the file a given tool actually
references, so I can read or edit the right file in its real location without
leaving the app.

## Scope

### In scope

- A hidden-until-hover "more actions" (`⋯`) menu on each capability row.
- "Open original" — opens the source file with the user's preferred editor.
  - Skill / hook: the marker file inside the folder (`SKILL.md` / `hook.json`).
  - Agent / rule: the capability file itself.
- "Reveal in Finder" — reveals the original file in the system file explorer.
- "Open in <Tool>" — one entry per enabled tool, opening the file that tool
  actually references (`ToolCapabilityState.targetPath`): the symlink/managed
  copy, or the instruction file for markdown-section rules.
- A Config dropdown to choose the preferred editor: System default / VS Code /
  Cursor / Custom (app name or path).

### Out of scope

- In-app file preview or editing (this is a manager, not an editor).
- Per-row open for disabled tools (only `enabled` projections are offered).
- Opening arbitrary paths — every path is validated against known roots.

## How it works

Opening is done entirely in the Rust core/shell, never from the WebView:

1. The UI calls `cmd_open_path({ path, openWith })` or `cmd_reveal_path({ path })`.
2. The command tilde-expands the path and asks
   `agentic_core::open_targets::is_openable` whether it canonicalizes under a
   configured source root, a tool target path (skills/agents/rules dir, or the
   exact instruction / hooks file), or a known workspace directory.
3. On pass, it calls `tauri-plugin-opener` (`open_path` / `reveal_item_in_dir`)
   from Rust. The WebView holds no opener or FS scope.

The preferred editor is stored as `Settings.editor` (`EditorPref { kind,
customApp }`) and mapped to an opener `openWith` app name via
`EditorPref::app_name()` (mirrored in the UI for the `openWith` argument).

## Security

`tauri-plugin-opener` is the official, scoped successor to the (forbidden)
`shell` plugin's `open` API. It cannot run arbitrary commands. The path
allowlist in `open_targets` is the fine gate; see
[ARCHITECTURE.permissions.md](../../ARCHITECTURE.permissions.md).

## Acceptance criteria

- [ ] Each row exposes a `⋯` menu, hidden until row hover.
- [ ] "Open original" opens the source file with the configured editor (or OS
      default when set to System default).
- [ ] Skill/hook rows open the marker file; agent/rule rows open the file itself.
- [ ] "Reveal in Finder" reveals the original file.
- [ ] One "Open in <Tool>" entry appears per tool whose projection is `enabled`,
      opening that tool's referenced file.
- [ ] Opening a path outside every known root fails with `path_not_openable`.
- [ ] The editor preference persists in `~/.agentic-hub/config.json`.

## Dependencies

- `tauri-plugin-opener` (Rust plugin + init); no WebView capability needed.
- New `agentic-core::open_targets::is_openable` validator.
- New IPC commands `cmd_open_path` / `cmd_reveal_path`.
- `Settings.editor` (`EditorPref`) + Config `EditorPanel`.
