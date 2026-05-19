# Getting Started

Status: Active
Mode: Detailed
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md)
Related Docs: [docs/tech/development/testing-strategy.md](./testing-strategy.md)

## Purpose

How to set up, build, and run Agentic Hub locally during development.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust toolchain | stable 1.78+ | Install via `rustup` |
| Node.js | 20.x LTS | Required by Vite + Tauri CLI |
| pnpm | 9.x | Package manager (pinned in `package.json#packageManager`) |
| Tauri CLI | 2.x | `cargo install tauri-cli --version "^2.0"` |
| Xcode CLT (macOS) | latest | Required for Tauri's WebView toolchain |
| WebKitGTK + build-essential (Linux) | distro-current | See Tauri 2.x prerequisites |

Verify:

```bash
rustc --version       # >= 1.78
node --version        # v20.x
pnpm --version        # 9.x
cargo tauri --version # 2.x
```

## Clone and bootstrap

```bash
git clone <repo-url> agentic-hub
cd agentic-hub
pnpm install
cargo fetch
```

## Repo layout (relevant for development)

```
agentic-hub/
  package.json              # pnpm workspace root
  pnpm-workspace.yaml
  Cargo.toml                # cargo workspace root
  crates/
    agentic-core/           # domain crate (no Tauri dependency)
    agentic-hub/             # Tauri bin crate; depends on agentic-core
  src/                      # React + Vite UI
  src-tauri/                # Tauri config + capability files
  resources/agentic-demo/    # bundled demo scaffold tree
  tests/integration/        # cross-crate FS integration tests
```

## Common scripts

All scripts run from the repo root.

### Dev (auto-reload UI + Tauri shell)

```bash
pnpm tauri dev
```

This:
- Runs `vite dev` for the UI on `localhost:5173`
- Builds and launches the Tauri shell linked to the dev UI
- Hot-reloads UI changes
- Recompiles Rust on save with auto-restart

### Build a release `.dmg` / `.deb` / `.AppImage`

```bash
pnpm tauri build
```

Output:
- macOS: `src-tauri/target/release/bundle/dmg/*.dmg`
- Linux: `src-tauri/target/release/bundle/{deb,appimage}/*`

### Lint

```bash
pnpm lint           # ESLint over src/
cargo clippy --all  # Rust lints
```

### Format

```bash
pnpm format         # Prettier
cargo fmt --all     # rustfmt
```

### Type codegen (Rust → TS)

```bash
cargo test -p agentic-core --features=ts-export
```

This regenerates `src/types/generated/*.ts` files. Run before commits that touch shared types.

### Tests

See [testing-strategy.md](./testing-strategy.md). Quick summary:

```bash
cargo test --workspace                   # Rust unit + integration
pnpm test                                # Vitest (UI logic)
pnpm test:e2e                            # WebDriver smoke (when enabled)
```

## Workflow patterns

### Adding a new IPC command

1. Define the input / output types in `agentic-core` with `#[derive(serde::*, ts_rs::TS)]`
2. Implement the handler in `agentic-core`
3. Expose a `#[tauri::command]` wrapper in `agentic-hub` (`src/commands.rs`)
4. Register the command in `tauri::Builder::default().invoke_handler(...)`
5. Add the command to `src-tauri/capabilities/default.json` permission list
6. Regenerate TS types: `cargo test -p agentic-core --features=ts-export`
7. Add a typed wrapper in `src/ipc/<concern>.ts`
8. Use it in React

### Adding a new tool adapter

1. Add the tool id to the `ToolId` enum in `agentic-core`
2. Add default settings in `agentic-core::settings::defaults`
3. Add the adapter logic in `agentic-core::adapter_registry`
4. Update `docs/tech/reference/tool-adapter-matrix.md`
5. Add the tab in the UI's tool-tabs component
6. Add adapter-specific tests in `crates/agentic-core/src/adapter_registry/tests.rs`

### Adding bundled demo content

1. Drop files into `resources/agentic-demo/<path>`
2. Rebuild — `include_dir!` re-embeds at compile time
3. Verify with a manual `cmd_scaffold_demo({ mode: 'overwrite' })` against a tempdir

## Tauri CLI tips

| Need | Command |
|------|---------|
| Run only the UI (Vite, no Tauri shell) | `pnpm dev` (defined as `vite dev` in `package.json`) |
| Build only the Rust shell | `cargo build -p agentic-hub` |
| Inspect capability file resolution | `cargo tauri info` |
| Generate icon set from source | `cargo tauri icon resources/icon.png` |

## Logging during dev

Set `RUST_LOG` to control core logging:

```bash
RUST_LOG=agentic_core=debug pnpm tauri dev
```

UI logs to the WebView dev tools (right-click → Inspect, available only in dev builds).

## Updating dependencies

```bash
pnpm update --interactive --latest    # UI
cargo upgrade --workspace             # Rust (requires cargo-edit)
```

Run the full test suite after dep updates.

## Release flow (outline)

1. Bump version in `crates/agentic-hub/Cargo.toml`, `package.json`, and `src-tauri/tauri.conf.json`
2. Update `CHANGELOG.md`
3. `pnpm tauri build`
4. Sign the macOS artifact (Apple Developer cert, if available)
5. Tag the release in git
6. Upload artifacts to the release page

## Troubleshooting

### "Failed to load Tauri capability file"

Check `src-tauri/capabilities/default.json` is well-formed JSON and that every command used by the UI is listed.

### "Permission denied" creating symlinks on Linux

The user running the dev binary must have write access to the target tool homes. Most issues are due to mis-configured `skills_path` etc. pointing at a path the user does not own. Verify with `cmd_load_settings` and adjust paths.

### Hot reload not picking up Rust changes

`pnpm tauri dev` watches `src-tauri/` and `crates/`. If changes are not picked up, check the terminal for compile errors. If the binary is panicking on startup, the watcher may not restart cleanly — kill and re-run.

### `ts-rs` type drift

If TS types in `src/types/generated/` disagree with Rust, you forgot to run `cargo test -p agentic-core --features=ts-export`. Run it and commit the regenerated files.

## Pointers

- Tauri 2.x docs: https://v2.tauri.app
- Capability files: https://v2.tauri.app/security/capabilities/
- Rust IPC: https://v2.tauri.app/develop/calling-rust/
- Plugins used: `tauri-plugin-dialog`, `tauri-plugin-store`
