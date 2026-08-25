# agentic-hub

Tauri 2.x desktop app that manages shared agentic capabilities — skills, agents, rules — across Codex, Claude Code, Cursor, OpenClaw, the OpenStandard `~/.agents` root, and opt-in tools (Kiro, Copilot, Antigravity, Grok Build) from one window. Rust core (`agentic-core`) owns the projection engine; React + Vite + TypeScript UI is a thin view over typed Tauri IPC.

## Directory map

| Path | Purpose |
|------|---------|
| `crates/agentic-core/` | Pure domain crate: `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync`, `suite_store`, `workspace_inventory`, `scaffold`. No Tauri dependency. |
| `crates/agentic-hub/` | Tauri 2.x bin crate: `#[tauri::command]` wrappers, window mgmt, capability JSON files. Depends on `agentic-core`. |
| `src/` | React + Vite + TypeScript UI. `components/ui/` (shadcn primitives), `components/*` (feature views), `state/` (Zustand stores — all UI logic, unit-tested), `lib/utils.ts`, `index.css` (Tailwind v4 tokens). Main window + Suite Manager window. |
| `src-tauri/capabilities/` | Tauri 2.x capability JSON files. Scope = FS-restricted; `shell` plugin not loaded. |
| `resources/agentic-demo/` | Bundled demo scaffold tree, embedded via `include_dir!`. |
| `tests/integration/` | Cross-crate FS integration tests against tempdirs. |
| `docs/` | Detailed docs (features, tech.modules, tech.reference, tech.development). |
| `PRODUCT.md` | Product requirements. |
| `ARCHITECTURE.md` | Root architecture + three companion domain docs. |

## Top-level docs

- [PRODUCT.md](PRODUCT.md) — what we are building and why
- [DESIGN.md](DESIGN.md) — UI design system (tokens, components, do's/don'ts; the source of truth for styling)
- [ARCHITECTURE.md](ARCHITECTURE.md) — system design (start here for implementation)
- [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) — Tauri capability model + Windows symlink constraint
- [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md) — projection engine (the bulk of the system)
- [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md) — workspace patch hard-copy lifecycle
- [CHANGELOG.md](CHANGELOG.md) — cumulative version history (Keep a Changelog format)
- [RELEASE.md](RELEASE.md) — the **current release's** notes only; the release workflow publishes it verbatim as the GitHub Release body
- [docs/README.md](docs/README.md) — full doc index

## Tech stack

- Tauri 2.x (Rust core + WebView shell)
- Rust workspace: `agentic-core` (domain) + `agentic-hub` (bin)
- React 18 + Vite + TypeScript + Zustand
- Tailwind v4 + shadcn/ui (components-first UI; tokens in `DESIGN.md`)
- `lucide-react` for all icons (the only icon source — see `DESIGN.md` Iconography)
- pnpm 9.x, Rust stable, Node 20.x
- `ts-rs` for Rust → TS type codegen
- `tauri-plugin-dialog`, `tauri-plugin-store` (no `shell` plugin)

## Commands

| Command | Purpose |
|---------|---------|
| `pnpm tauri dev` | Dev server (Vite + Tauri shell, hot reload) |
| `pnpm tauri build` | Release bundle (.dmg / .deb / .AppImage) |
| `cargo test --workspace` | Rust unit + integration tests |
| `pnpm test` | Vitest unit tests for UI logic (`src/state/` stores) |
| `pnpm test:watch` | Vitest in watch mode (TDD loop) |
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

- Rust: **TDD is the default** for the core — write the failing test first (see [docs/tech/development/testing-strategy.md](docs/tech/development/testing-strategy.md)). Strict `clippy`, `cargo fmt`, no `unwrap()`/`expect()`/`panic!` outside tests (the Tauri `run()` entrypoint is the sole exception), errors via `thiserror` enums. Follow the `rust-best-practices` skill. The lint bar is enforced by `[workspace.lints]` in `Cargo.toml`, not by convention.
- TS: strict `tsconfig`, no `any`, types mirrored from Rust via `ts-rs` (never hand-edited)
- UI is **components-first**: compose shadcn primitives from `components/ui/`; never hand-roll a styled element when a primitive exists. Style only through `DESIGN.md` tokens (CSS variables), never ad-hoc colors.
- UI logic lives in Zustand stores under `src/state/`, not in components. Components stay thin (render + event wiring). The IPC layer (`src/ipc.ts`) is the only place that talks to Rust.
- File size cap: 600 lines per source file (Arno workspace convention)
- Comments explain non-obvious intent; never narrate what the code does

## Testing & TDD

> Tests are the cheapest safety net. Logic with defined inputs → outputs is tested; visual layout is verified by eye.

- **Two suites, one discipline.** The Rust core uses `cargo test` (unit + integration); the UI uses Vitest over the `src/state/` stores. Both stay green before any change lands.
- **TDD is the default for logic** (stores, planners, pure helpers): Red → Green → Refactor. Write the failing test first, make it pass with the minimum, then clean up. Skip TDD for pure layout/styling and framework wiring — verify those visually.
- **Prove-it for bugs.** Reproduce a bug as a failing test *before* fixing it; the passing test is the proof and the regression guard. "Seems fixed" is not done.
- **Test the behavior, not the internals.** Assert on store outputs and observable state (`getState()`), not private helpers. If a refactor that preserves behavior breaks a test, the test was too coupled.
- **Mock at the boundary only.** Store tests mock the Tauri IPC module (`@/ipc`) and Sonner toasts; everything else runs for real. Reset each store between tests with `setState(getInitialState(), true)`. Keep tests in Node env — no DOM needed for store logic.
- **Name by scenario, one behavior per test** (`"reload falls back to the first target when no active id is persisted"`), and follow Arrange-Act-Assert. Prefer DAMP (readable, self-contained) over DRY in tests.

## When making changes

1. Identify the right module (use `docs/README.md` "how to read this" guide)
2. Read the matching `docs/tech/modules/*.md` before touching code
3. Touch the Rust core first if the change is FS-related; touch UI only after the IPC contract is stable
4. For core logic, **start with a failing test** (Red → Green → Refactor); for bug fixes, write the reproducing test before the fix
5. Regenerate TS types if Rust shared types changed
6. Run `cargo test --workspace` and `cargo clippy --all-targets --all-features --locked -- -D warnings` — both must stay green. If you touched `src/state/`, add/extend the Vitest store tests first and run `pnpm test`
7. Update the matching `docs/` if behavior or contract changes

## Hard rules

- Markdown managed-block markers stay verbatim: `<!-- agentic-hub:start -->` / `:end` (heading `## Agentic Hub Managed Rules`). Migration parity with the rebranded VS Code extension. Managed-copy metadata lives in a per-root `.agentic-hub-managed.json` manifest; hook entries carry the `_agenticHub` marker keyed by the bare manifest id. Do not reintroduce the legacy `e-studio-*` names.
- Suite storage path stays `~/.agentic-suites.json`. Same reason.
- Workspace scope is **read-only** — it scans a project's own tool dirs and reports what each tool already has. The scan never writes. Do not reintroduce a workspace apply / patch / manifest path. **One opt-in exception:** when the skills.sh source is enabled, the user may explicitly install a starred skill into a workspace via `cmd_install_skill`. This is the *only* workspace write path — explicit, user-initiated, never automatic, never part of scanning. It runs the source CLI via a controlled `std::process::Command` (validated `owner/repo` ref, cwd = the remembered workspace dir); the hub keeps no install state and re-scans the read-only inventory afterward. Still **no `tauri-plugin-shell`** (a login shell is used only to read `PATH`, never to run the install). See [docs/tech/modules/skill-sources.md](docs/tech/modules/skill-sources.md).
- `RELEASE.md` holds **only the current release's** notes. The release workflow publishes it verbatim as the GitHub Release body (`body_path: RELEASE.md` in `.github/workflows/release.yml`), so any older versions left in the file show up on every release. When cutting a release, **replace** `RELEASE.md` with the new version's notes — never prepend. The cumulative history lives in `CHANGELOG.md` (append there, newest first).
- `tauri-plugin-shell` is never added to `Cargo.toml`. If a feature seems to need it, raise security review first.
- Every IPC command must appear in `src-tauri/capabilities/default.json`.
- Every path parameter is canonicalized via the central validator before any FS op.
- New `agentic-core` behavior lands test-first (Red → Green → Refactor). The thin `#[tauri::command]` wrappers in `agentic-hub` are marshalling-only and exempt — keep logic in `agentic-core` where it can be unit-tested. Never weaken `[workspace.lints]` or `#[allow]` a lint to land code; fix the cause or use a documented local `#[expect(...)]`.

## Migration from VS Code extension

This product is a Tauri-native port of the Unified Agentic Capability Manager originally shipping inside `e-studio-copilot/packages/vs-code/`. The product semantics are preserved 1:1:

- Same `~/.agentic` shared root contract
- Same per-tool projection rules (flat for Claude skills, managed copy for Cursor agents, markdown section for Codex/Claude/OpenClaw rules)
- Same `~/.agentic-suites.json` for suites
- Same managed-block markers
- Workspace scope is the one intentional divergence: the VS Code extension hard-copied suites into a project; the hub instead treats each workspace as a read-only inventory of what its tools already have.

Users coming from the VS Code extension keep working without reconfiguring their tools.
