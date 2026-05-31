---
name: Run, Test & Debug
slug: run-test-debug
---

# Run, Test & Debug

A practical, verified cheat sheet for starting Agentic Hub, running its checks,
and debugging when something breaks. Every command here is confirmed against
the current repo. For first-time setup and prerequisites, see
[getting-started.md](getting-started.md).

## TL;DR

```bash
pnpm install              # once
pnpm tauri dev            # run the full desktop app (hot reload)
cargo test --workspace    # run all Rust tests
pnpm build                # type-check + build the UI
```

## Toolchain (already satisfied on this machine)

| Tool | Needed | Note |
|------|--------|------|
| Node | 20.x+ | `node@22` is on `PATH` |
| pnpm | 9.x+ | package manager |
| Rust | stable 1.78+ | `cargo` |
| Tauri CLI | 2.x | **local** dep — use `pnpm tauri`, no global install |

The Tauri CLI ships as the `@tauri-apps/cli` dev dependency, so `pnpm tauri …`
just works. You do **not** need `cargo install tauri-cli`.

> Gotcha: in an interactive shell, `node` is an nvm lazy-load function and may
> fail if no nvm default is set. It does not affect `pnpm`/`vite`/`tauri`, which
> resolve the real `node@22` binary on `PATH`. If you call `node` directly and
> it errors, use `/usr/bin/env node` or `nvm use 22`.

## Ways to start the app

### 1. Full desktop app (the normal way)

```bash
pnpm tauri dev
```

What it does, in order:

1. Runs `pnpm dev` (Vite) on `http://localhost:1420` (fixed, strict port).
2. Compiles the Rust shell (`crates/agentic-hub`) — first build is slow.
3. Opens the native window and watches `crates/` + `src/` for changes.

Hot reload: editing `src/**` refreshes the WebView instantly; editing
`crates/**/*.rs` recompiles and relaunches the shell.

### 2. UI only, in a browser (no Rust shell)

```bash
pnpm dev        # then open http://localhost:1420
```

Fast for pure layout/styling work. Any `invoke("cmd_*")` call will fail (there
is no Tauri backend), so this is UI-shape only — not for testing IPC flows.

### 3. Release binary

```bash
pnpm tauri build
```

Currently compiles a release binary but produces **no installer**: bundling is
off (`"bundle": { "active": false }` in `crates/agentic-hub/tauri.conf.json`).
To produce a `.dmg`/`.deb`/`.AppImage`, set `bundle.active` to `true` first.

## Test & verify

Run these before committing. All four are expected to pass clean.

| Goal | Command |
|------|---------|
| Rust unit + integration tests | `cargo test --workspace` |
| Rust lint (deny warnings) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust format | `cargo fmt --all` |
| UI type-check + build | `pnpm build` |
| Regenerate Rust → TS types | `pnpm gen:types` |

Run a single core module's tests:

```bash
cargo test -p agentic-core hook_sync       # by module name filter
cargo test -p agentic-core -- --nocapture  # show println! output
```

### Type-drift check

After changing any shared type in `agentic-core`, regenerate and confirm the
generated TS matches what's committed:

```bash
pnpm gen:types
git status --porcelain -- src/types/generated/   # empty = no drift
```

If that prints changed files, commit them. Never hand-edit
`src/types/generated/`.

## Debug

### Logs

The Rust core honors `RUST_LOG`:

```bash
RUST_LOG=agentic_core=debug pnpm tauri dev
```

UI logs go to the WebView dev tools: right-click in the window → **Inspect**
(dev builds only).

### Reading the dev server output

`pnpm tauri dev` streams Vite + cargo + the running binary to its terminal.
Watch for these markers:

- `VITE … ready` + `Local: http://localhost:1420/` — UI server is up.
- `Compiling agentic-hub` → `Finished` → `Running …/target/debug/agentic-hub`
  — shell built and launched.
- A Rust `panic`/`thread '…' panicked` line — startup or command crash.

### Common failures

| Symptom | Cause / fix |
|---------|-------------|
| `Port 1420 is already in use` | A stale `vite`/`tauri dev` is running. Kill it (`pkill -f "tauri dev"`) and restart. |
| Window opens blank / white | Vite not reachable; check the `VITE ready` line and that nothing else holds `:1420`. |
| `Failed to load capability` | A used command is missing from `crates/agentic-hub/capabilities/default.json`. Add it. |
| `command … not found` (IPC) | Command not registered in the `invoke_handler![…]` list in `crates/agentic-hub/src/lib.rs`. |
| TS type errors after Rust change | You forgot `pnpm gen:types`. Run it and rebuild. |
| Permission denied creating symlinks | The target tool home isn't writable; check the configured paths in settings. |
| Rust hot reload not restarting | Compile error in the terminal, or a panic on startup — fix it, then re-run `pnpm tauri dev`. |

### Stop / restart

```bash
# Stop: Ctrl+C in the dev terminal, or:
pkill -f "tauri dev"
pkill -f "target/debug/agentic-hub"
```

## What the app touches (for debugging state)

Agentic Hub mutates only the local filesystem — no network. When verifying
behavior, inspect these:

| Path | Holds |
|------|-------|
| `~/.agentic/` | Default shared source root (scanned) |
| `~/.codex/`, `~/.claude/`, `~/.cursor/`, `~/.openclaw/` | Per-tool projection targets |
| `~/.agentic-suites.json` | Saved suites |
| `~/.agentic-hub/state.json` | Tracked workspace targets |
| `<root>/.agentic-hub-managed.json` | Managed-copy manifest per target dir |
| `<ws>/.agentic-hub/` | Workspace-patch payload for a project |

If the matrix is empty, your source root simply has no capabilities yet — that
is expected, not an error.
