# Architecture: Permissions

This document deepens the Tauri 2.x permission and trust-boundary design for Agentic Hub. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for the full system view.

## Context from root architecture

The root architecture establishes two trust zones: the **untrusted WebView** (React UI) and the **trusted Rust core** (`agentic-core`). Tauri 2.x's capability system is the mechanism that enforces this boundary at the IPC layer. This doc owns the capability model, FS scope rules, and platform-specific filesystem constraints (notably Windows symlinks).

## Why this domain is split out

Tauri 2.x's permission model is meaningfully different from Tauri 1.x's allowlist. It uses fine-grained capability JSON files that scope which commands and which paths a given window can access. For an app that writes into multiple sensitive tool homes (`~/.codex`, `~/.claude`, `~/.cursor`, `~/.openclaw`) and accepts user-picked workspace dirs at runtime, the capability story is itself a substantial design surface. Folding it into the root architecture would either bloat the root doc or under-specify a critical safety surface.

This doc also owns the cross-platform symlink story because symlink semantics interact directly with the FS scope model and dictate which platforms can ship which features in v1.

## Goals

- Define the static capability surface declared in `src-tauri/capabilities/*.json`
- Define how user-picked workspace directories add runtime scope
- Define what the WebView cannot do (negative capabilities)
- Document the Windows symlink constraint and the managed-copy fallback path
- Provide a checklist for reviewing capability files before each release

## Non-goals

- Auto-update signing (deferred to launch)
- Code signing for macOS Gatekeeper (deferred to launch)
- Sandboxing the Rust core (out of scope; Rust core is fully trusted)
- Cross-app sandboxing (e.g. restricting what Codex itself can read from `~/.agents/skills`) — Agentic Hub does not enforce what the target tools do with the projected files

## Scope and boundaries

In scope:
- Tauri capability files (`src-tauri/capabilities/`)
- Allowed Tauri plugins and their scope (fs, dialog, store, updater)
- Disabled plugins (shell)
- Path validation rules inside the Rust core
- Windows symlink behavior and fallback

Out of scope:
- The IPC payload schema itself (see [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md))
- The plan/apply business logic (see [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md))

## Existing system and reuse

Tauri 2.x ships with these plugins relevant to this app:

- `tauri-plugin-fs` — scoped FS access from the WebView (we use it minimally; almost all FS work happens in `agentic-core`)
- `tauri-plugin-dialog` — native folder picker (needed for workspace selection)
- `tauri-plugin-store` — persistent key-value store (used for `state.json` workspace target LRU)
- `tauri-plugin-shell` — explicitly **disabled** (the WebView must never run arbitrary commands)
- `tauri-plugin-updater` — deferred to post-v1

The Rust core uses `std::fs`, `std::os::unix::fs::symlink` (Unix), `std::os::windows::fs::{symlink_file, symlink_dir}` (Windows), and `fs_extra` for recursive copy.

## Trust zones

```
+--------------------+              +-----------------------+
|   WebView (UI)     |   IPC only   |   Rust core           |
|   - React          | <----------> |   - all FS mutations  |
|   - never touches  |              |   - all path validate |
|     filesystem     |              |   - all symlink ops   |
|   - typed invoke() |              |   - tool home writes  |
+--------------------+              +-----------------------+
        ^                                       ^
        |                                       |
   capability JSON                          OS file permissions
   (allowlist of                            (root for nothing;
    invokable cmds)                          user-level access)
```

The WebView never holds a filesystem handle. Every command parameter is validated server-side. A compromised WebView (e.g. via a malicious skill file rendered in the inspector) cannot escalate to FS writes outside the scoped paths, and cannot run shell commands at all.

## Capability files

Tauri 2.x reads capability JSON files from `src-tauri/capabilities/`. Each capability declares: which windows it applies to, which IPC commands are permitted, and (for FS / dialog) which paths are scoped.

### `capabilities/default.json` — base capabilities for all windows

Permitted:

- Custom commands defined in `agentic-hub` bin crate (the full `cmd_*` surface — see [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md))
- `core:event:default` — listen to events emitted from Rust
- `core:window:default` — basic window controls (close, minimize, drag)
- `dialog:default`, `dialog:allow-open` — folder picker
- `store:default` — KV store access for `state.json`

Denied / not granted:

- `shell:*` — never
- `fs:*` direct from the WebView — instead, all FS work goes through our custom commands which enforce path validation internally
- `os:*` beyond `core:os:default` — no env-var reads from the WebView

### `capabilities/workspace.json` — runtime scope additions

When the user picks a workspace directory via the dialog plugin, the Rust core stores that directory and exposes commands scoped to it. The capability file does **not** statically grant access to `~/Code/**`. Instead, the path validation lives in `agentic-core::workspace_patch`:

```rust
fn validate_workspace_target(target: &Path, workspace_root: &Path) -> Result<()> {
    let real_target = target.canonicalize()?;
    let real_ws = workspace_root.canonicalize()?;
    if !real_target.starts_with(&real_ws) {
        return Err(Error::OutOfWorkspace(target.to_owned()));
    }
    Ok(())
}
```

Every write inside `workspace_patch::apply` calls this validator. The capability layer is a coarse gate; the core is the fine gate.

### Static FS scope for tool homes

Tool homes are predictable. They live under the user's home directory at well-known paths. The `agentic-core::adapter_registry` resolves these at runtime via `dirs::home_dir()`. The capability layer does not statically restrict tool-home access because the user may configure non-default `skillsPath` / `agentsPath` / etc. — and we want those to work.

The safety net: every write inside `applier` and `rule_sync` resolves the target to an absolute canonical path, refuses to overwrite real files / dirs, and refuses to write outside the tool's configured target paths. The Rust core enforces this; the capability file does not.

## Plugin configuration

### `tauri-plugin-fs`

Configured minimally. The WebView is not granted `fs:allow-read` or `fs:allow-write` against any path by default. The only FS-adjacent capability the WebView holds is the dialog plugin, which returns a path string the user explicitly chose.

If a future feature needs WebView-side FS read (e.g. showing a rule body preview in a `<pre>` block), the preferred approach is to add a `cmd_read_capability_body(item_id)` command in `agentic-core` rather than granting the WebView raw `fs:allow-read`.

### `tauri-plugin-dialog`

Granted `dialog:allow-open` for folder selection. Used by the workspace target picker.

### `tauri-plugin-store`

Granted `store:default` access scoped to `state.json` under the app data directory (`~/.agentic-hub/state.json` on macOS/Linux, `%APPDATA%/agentic-hub/state.json` on Windows). Used for the workspace target LRU and last-active tool tab.

### `tauri-plugin-shell` — disabled

Not added to `Cargo.toml`. Not declared in capability files. The WebView cannot spawn processes.

### `tauri-plugin-updater` — deferred

Will be added in a post-v1 release once a signed manifest endpoint exists. Until then, users update by downloading a new `.dmg` / `.deb`.

## Path validation rules

Every command that accepts a path parameter validates it before any FS op. The rules are:

1. **Tilde expansion.** `~/foo` resolves to the user's home directory at the start of validation, never inside FS ops. Mixed-case `~` handling is OS-specific; we always expand via `dirs::home_dir()`.
2. **Canonicalization.** `path.canonicalize()` is called once, early. Symlinks in the path are resolved. Errors here propagate as `path_not_found` or `path_invalid`.
3. **No path traversal.** `..` segments are not stripped; canonicalization handles them. After canonicalization, `starts_with` checks compare against the canonicalized parent.
4. **Tool-home scope.** Apply ops verify the resolved target lives under one of `{skills,agents,rules,instructions}_path` for the focused tool. Writes outside this scope are refused.
5. **Workspace scope.** Workspace patch ops verify the resolved target lives under `realpath(workspace_dir)`. See above.
6. **Real-file guard.** Before any `replace_link` / `replace_managed_copy` / `remove_link` op, the target is `lstat`-checked. A regular file or directory blocks the op (`skip_conflict`).

## Windows symlink constraint

On Windows, creating symlinks requires either:

- Developer Mode enabled in Windows Settings, or
- The user running the app as administrator, or
- The user holding `SeCreateSymbolicLinkPrivilege`

None of these are reliable assumptions for a personal-tool app. For v1, the design rules are:

- **macOS and Linux are primary.** Symlinks work via `std::os::unix::fs::symlink`.
- **Windows is documented as constrained.** v1 does not officially ship Windows binaries.
- **Managed-copy fallback path.** The `applier` and `workspace_patch` code paths that use managed copies (Cursor agents in global scope, every projection in workspace scope) work on Windows in principle. A future Windows release can opt every tool into managed-copy projection by setting `ruleProjectionKind = file_sync` for tools that currently use `link_sync`, and overriding the `link_sync` agent path to use `create_managed_copy`. This is a config-only change to the adapter layer.
- **Stale detection still works.** Managed-copy sync metadata stores the source path + content hash; refresh-on-source-change works identically on Windows.

If we ship Windows in a later release, the symlink path must be guarded:

```rust
fn create_symlink(source: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)] {
        std::os::unix::fs::symlink(source, target).map_err(Error::from)
    }
    #[cfg(windows)] {
        let attempted = if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, target)
        } else {
            std::os::windows::fs::symlink_file(source, target)
        };
        attempted.or_else(|_| fall_back_to_managed_copy(source, target))
    }
}
```

This is documented for future work, not implemented in v1.

## Negative capabilities

The WebView cannot:

- Run shell commands (`tauri-plugin-shell` not loaded)
- Read or write arbitrary files (no `fs:allow-*` granted)
- Read environment variables beyond what Tauri exposes by default
- Spawn child windows except via documented commands
- Open external URLs (no `shell:allow-open` granted; if we need to open a file in the user's editor, a dedicated `cmd_open_in_editor(path)` is added with explicit validation)
- Bypass the per-command argument validation in the Rust core

## Operational rules

- Every release: review every capability file and confirm no `*:allow-*` was added without an explicit security rationale in the commit message
- Every new IPC command: review its payload validation in `agentic-core` before merge
- Every new path parameter: confirm it goes through the validator before any FS op
- `tauri-plugin-shell` must remain absent from `Cargo.toml`. If a future feature seems to need it, raise that as a security design discussion, not a quiet dependency addition

## Related detailed docs

- [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md) — full command surface and event schemas
- [docs/tech/modules/workspace-patch.md](docs/tech/modules/workspace-patch.md) — workspace path validation in detail

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| Capability denial | UI invokes a command not in its capability file | Tauri rejects the IPC call | Bubble error to UI as `permission_denied`; user reports as a bug |
| Path canonicalization | Target path does not exist | Validator returns `path_not_found` | Surface in UI; user creates parent or fixes settings |
| Workspace scope violation | Resolved target escapes workspace root | `workspace_patch::apply` records error for that path | Continue with remaining items; report in result summary |
| Tool-home scope violation | Apply tries to write outside configured tool paths | `applier` refuses | Surface as bug (this should never happen if adapters are correct) |
| Windows symlink fail (future) | `symlink_file` returns `ERROR_PRIVILEGE_NOT_HELD` | Fall back to managed copy if implemented; otherwise error | Document Developer Mode requirement in install docs |
| Plugin store write fail | Disk full / permission | LRU state lost | Surface; in-memory state preserved until next app start |

## Open questions

- Should we ship a `cmd_open_in_editor(path)` that uses `tauri-plugin-shell::open` to open a file in the user's default editor? Useful for "edit this rule" affordance. Decision: yes, but with explicit allowlist of file extensions and a path validator that confirms the path is under the shared root or a known tool home. Defer to M2 polish.
- Should we add `tauri-plugin-fs` with a `fs:scope` of `~/.agentic/**` (read-only) so a future "preview rule body" feature can render markdown without a custom command? Decision: no in v1. Custom command keeps the boundary clean.
- macOS notarization: required for distributing the `.dmg` without Gatekeeper warnings. Decide at M4 whether to invest in an Apple Developer account for this personal-tool app, or document the right-click-open workaround in install docs.
