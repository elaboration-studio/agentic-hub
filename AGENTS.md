# agentic-hub

Tauri 2.x desktop app that manages shared agentic capabilities — skills, agents, rules — across Codex, Claude Code, Cursor, and OpenClaw from one window. Rust core (`agentic-core`) owns the projection engine; React + Vite + TypeScript UI is a thin view over typed Tauri IPC.

## Directory map

| Path | Purpose |
|------|---------|
| `crates/agentic-core/` | Pure domain crate: `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync`, `suite_store`, `workspace_patch`, `scaffold`. No Tauri dependency. |
| `crates/agentic-hub/` | Tauri 2.x bin crate: `#[tauri::command]` wrappers, window mgmt, capability JSON files. Depends on `agentic-core`. |
| `src/` | React + Vite + TypeScript UI. Main window + Suite Manager window. |
| `src-tauri/capabilities/` | Tauri 2.x capability JSON files. Scope = FS-restricted; `shell` plugin not loaded. |
| `resources/agentic-demo/` | Bundled demo scaffold tree, embedded via `include_dir!`. |
| `tests/integration/` | Cross-crate FS integration tests against tempdirs. |
| `docs/` | Detailed docs (features, tech.modules, tech.reference, tech.development). |
| `PRODUCT.md` | Product requirements. |
| `ARCHITECTURE.md` | Root architecture + three companion domain docs. |

## Top-level docs

- [PRODUCT.md](PRODUCT.md) — what we are building and why
- [ARCHITECTURE.md](ARCHITECTURE.md) — system design (start here for implementation)
- [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) — Tauri capability model + Windows symlink constraint
- [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md) — projection engine (the bulk of the system)
- [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md) — workspace patch hard-copy lifecycle
- [docs/README.md](docs/README.md) — full doc index

## Tech stack

- Tauri 2.x (Rust core + WebView shell)
- Rust workspace: `agentic-core` (domain) + `agentic-hub` (bin)
- React 18 + Vite + TypeScript + Zustand
- pnpm 9.x, Rust stable, Node 20.x
- `ts-rs` for Rust → TS type codegen
- `tauri-plugin-dialog`, `tauri-plugin-store` (no `shell` plugin)

## Commands

| Command | Purpose |
|---------|---------|
| `pnpm tauri dev` | Dev server (Vite + Tauri shell, hot reload) |
| `pnpm tauri build` | Release bundle (.dmg / .deb / .AppImage) |
| `cargo test --workspace` | Rust unit + integration tests |
| `pnpm test` | Vitest UI tests |
| `cargo test -p agentic-core --features=ts-export` | Regenerate TS types from Rust |
| `cargo clippy --all && pnpm lint` | Lint everything |
| `cargo fmt --all && pnpm format` | Format everything |

See [docs/tech/development/getting-started.md](docs/tech/development/getting-started.md) for the full workflow.

## Core philosophy

> The Rust core owns every filesystem mutation. The WebView is untrusted.

- Filesystem state is the source of truth. No shadow database.
- Plan-then-apply is mandatory. Stage in memory, plan from disk, apply explicitly.
- Apply is partial-tolerant. One failing op never aborts the rest.
- Never overwrite real files or directories. Conflicts surface as `skip_conflict`, never silent.
- Symlink semantics live in one place (`applier`). Layout decisions live in one place (`adapter_registry`).

## Code style

- Rust: strict `clippy`, `cargo fmt`, no `unwrap()` outside tests, errors via `thiserror` enums
- TS: strict `tsconfig`, no `any`, types mirrored from Rust via `ts-rs` (never hand-edited)
- File size cap: 600 lines per source file (Arno workspace convention)
- Comments explain non-obvious intent; never narrate what the code does

## When making changes

1. Identify the right module (use `docs/README.md` "how to read this" guide)
2. Read the matching `docs/tech/modules/*.md` before touching code
3. Touch the Rust core first if the change is FS-related; touch UI only after the IPC contract is stable
4. Regenerate TS types if Rust shared types changed
5. Add tests at the right layer (see [docs/tech/development/testing-strategy.md](docs/tech/development/testing-strategy.md))
6. Update the matching `docs/` if behavior or contract changes

## Hard rules

- Markdown managed-block markers stay verbatim: `<!-- e-studio-agentic-rules:start -->` / `:end`. Migration parity with the VS Code extension. See [PRODUCT.md Open Questions](PRODUCT.md) before considering rename.
- Suite storage path stays `~/.agentic-suites.json`. Same reason.
- Workspace manifest folder is `<ws>/.agentic-hub/`.
- `tauri-plugin-shell` is never added to `Cargo.toml`. If a feature seems to need it, raise security review first.
- Every IPC command must appear in `src-tauri/capabilities/default.json`.
- Every path parameter is canonicalized via the central validator before any FS op.

## Migration from VS Code extension

This product is a Tauri-native port of the Unified Agentic Capability Manager originally shipping inside `e-studio-copilot/packages/vs-code/`. The product semantics are preserved 1:1:

- Same `~/.agentic` shared root contract
- Same per-tool projection rules (flat for Claude, managed copy for Cursor agents, markdown section for Codex/Claude/OpenClaw rules)
- Same `~/.agentic-suites.json` for suites
- Same managed-block markers
- Workspace manifest folder renamed from `.e-studio-copilot/` to `.agentic-hub/` (the only breaking change)

Users coming from the VS Code extension keep working without reconfiguring their tools.
