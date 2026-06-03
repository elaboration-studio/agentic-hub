# Testing Strategy

Status: Active
Mode: Detailed
Last Updated: 2026-06-02
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/development/getting-started.md](./getting-started.md)

## Purpose

Define how Agentic Hub is tested across Rust core, Tauri shell, React UI, and end-to-end flows. The goal: every projection-layer change is covered by at least one test that exercises a real filesystem, and every IPC command is covered by at least one cross-layer test.

## TDD is the default for the Rust core

`agentic-core` is developed **test-first**. The core owns every filesystem mutation, so its behavior must be pinned by a test before it exists.

**Red → Green → Refactor**

1. **Red** — write the smallest failing test describing the next behavior. Run it; confirm it fails for the right reason.
2. **Green** — write the minimum code to pass. Run `cargo test --workspace`; everything passes.
3. **Refactor** — clean up production and test code while staying green.

**Bug fixes use the prove-it pattern**: write a test that reproduces the bug (it fails against current code), then fix until it passes. A passing reproduction test is the definition of done — "seems fixed" is not.

**Where TDD applies**

- **Always** — `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync`, `hook_sync`, `suite_store`, `workspace_inventory`, `workspace_target_store`, `scaffold`, `reconcile`, `settings`, `paths`, `managed_copy`, `open_targets`: all pure logic and tempfile-backed services with defined inputs → outputs.
- **Exempt** — the `#[tauri::command]` wrappers in `agentic-hub` are marshalling-only (payload in, `agentic-core` call, error map out). Keep logic out of them so it stays unit-testable in the core. They are covered by integration tests (below), not unit TDD.

**Enforced bar.** Every change keeps these green — they are wired into `[workspace.lints]`, so `cargo build`/`cargo test` fail on violations, not just CI:

```bash
cargo test --workspace
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Follow the `rust-best-practices` skill (borrowing over cloning, `Result` over panic, no redundant clones, idiomatic lifetimes).

## Test pyramid

```
                  +-----------------+
                  |   E2E (WebDriver)|     few; smoke only
                  +-----------------+
                /                      \
         +-------------------------------+
         |  Integration (tempfile + IPC) |  most projection-layer tests
         +-------------------------------+
       /                                  \
+--------------------+              +---------------------+
| Rust unit          |              | UI unit (Vitest)    |
| (agentic-core)     |              | (component, store)  |
+--------------------+              +---------------------+
```

## Test layers

### 1. Rust unit tests (inline `#[cfg(test)] mod tests` per module)

In-process tests for pure functions and tempfile-backed services. Each `agentic-core` module carries its own `#[cfg(test)] mod tests` block at the bottom of the file (colocated with the code under test), backed by `tempfile` tempdirs. As of this writing the core has 86 such tests across 17 modules.

Coverage targets:
- `scanner`: validation rules, nested categories, symlink resolution, cycle detection
- `adapter_registry`: target path resolution, flat vs nested, layout decisions per `(tool, kind)`
- `planner::inspect`: state classification for each `LinkState` (enabled, disabled, broken, stale, foreign_file, foreign_link)
- `planner::build_plan`: every operation kind, including flat-layout collision pass
- `applier`: each operation kind under positive and negative conditions (real-file blocks, symlink replacement, managed-copy stale detection)
- `rule_sync`: managed-block contract (create, refresh, remove, malformed markers, frontmatter stripping)
- `suite_store`: CRUD, validation, atomic writes, malformed-dotfile handling
- `workspace_inventory`: per-tool discovery, cross-tool dedupe, instruction-file presence, empty workspace, `__archived__` skip
- `scaffold`: merge mode, overwrite mode, file-target guard

Tools:
- `tempfile` crate for tempdir fixtures (the only test dependency today)
- `pretty_assertions` (human-readable diffs) and `rstest` (parameterized cases) are optional adds — pull them in when a module's assertions or case matrix justify it

Example pattern:

```rust
#[test]
fn replaces_broken_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let shared = make_shared_root_with_skill(&tmp, "skill-a");
    let tool_path = tmp.path().join("codex/skills");
    create_broken_link(tool_path.join("skill-a"));

    let plan = build_plan_for_skill(&shared, "skill-a", &tool_path, desired=true);
    let result = apply(&plan).unwrap();

    assert_eq!(result.replaced, 1);
    assert_eq!(fs::read_link(tool_path.join("skill-a")).unwrap(), shared.join("skills/skill-a"));
}
```

Run: `cargo test --workspace`

### 2. UI unit tests (`src/**/__tests__/*.test.ts`)

Vitest tests for React components and Zustand stores. No FS access; IPC is mocked.

Coverage targets:
- Capability list rendering for each state badge
- Staging model: toggle, clear, count
- Suite selector dropdown
- Error/empty state rendering
- IPC error handling

Tools:
- Vitest
- `@testing-library/react`
- `msw` or hand-rolled mocks for `invoke` / `listen`

Run: `pnpm test`

### 3. Integration tests (`tests/integration/*.rs`) — planned

> Status: not yet implemented. The contract below is the target; build it test-first when the IPC surface stabilizes.

Cross-crate tests that exercise IPC handlers end-to-end against a tempdir filesystem.

These tests do not boot a Tauri window; they call the command handlers directly via the `agentic-hub` crate's library export. The goal is to validate that the handler glue (input validation, error envelope, event emission) works on top of `agentic-core`.

Coverage targets:
- `cmd_scan` + `cmd_inspect` + `cmd_plan` + `cmd_apply` round trip
- `cmd_apply_suite` full reset
- `cmd_scan_workspace` read-only inventory of a workspace's tool dirs
- `cmd_sync_rules` managed-block lifecycle
- `cmd_scaffold_demo` materializes bundled tree

Tools:
- Tempdir fixtures
- A test helper that exposes `agentic-hub`'s command handlers as plain async functions

### 4. E2E smoke (`tests/e2e/*.ts`)

WebDriver-based smoke tests (Tauri's `tauri-driver`). Few, broad, slow. Run on CI for release builds only.

Coverage targets:
- App launches and shows the main window
- Empty-state scaffold button materializes the demo tree and the inventory renders
- Toggle + apply for one item produces a symlink at the expected path

Tools:
- `tauri-driver`
- `webdriverio`

## Mandatory tests before shipping a milestone

| Milestone | Required tests |
|-----------|----------------|
| M0 (foundation) | Settings load/save unit; Tauri shell launches with empty window in E2E |
| M1 (core loop) | Full Rust unit coverage of `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync`. Integration tests for every `cmd_*` in this milestone. E2E smoke for the empty-state → scaffold → apply happy path |
| M2 (suite presets) | `suite_store` unit; `cmd_apply_suite` integration; cross-window event broadcast verified |
| M3 (workspace inventory) | `workspace_inventory` unit (per-tool discovery, cross-tool dedupe, `__archived__` skip); `cmd_scan_workspace` integration on a tempdir project |
| M4 (launch hardening) | Run the full Rust test matrix on CI for macOS + Linux; manual smoke on a fresh macOS install |

## Filesystem fixtures

> Status: planned. Today each module builds its tempdir fixtures inline with small per-file helpers. Extract the shared builders below once duplication across modules justifies it.

Common fixtures will live in `crates/agentic-core/src/test_support/`:

```rust
pub struct SharedRootBuilder { /* ... */ }
impl SharedRootBuilder {
    pub fn skill(self, rel: &str) -> Self;
    pub fn agent(self, rel: &str, content: &str) -> Self;
    pub fn rule(self, rel: &str, body: &str) -> Self;
    pub fn build(self) -> PathBuf;  // returns tempdir path
}

pub struct ToolHomeBuilder { /* ... */ }
impl ToolHomeBuilder {
    pub fn correct_link(self, item: CapId) -> Self;
    pub fn broken_link(self, target_rel: &str) -> Self;
    pub fn wrong_link(self, target_rel: &str, pointing_at: &Path) -> Self;
    pub fn real_file(self, target_rel: &str, content: &str) -> Self;
    pub fn managed_copy(self, item: CapId, hash: &str) -> Self;
    pub fn instruction_file(self, kind: ToolId, content: &str) -> Self;
    pub fn build(self) -> PathBuf;
}
```

These keep individual tests short and let us describe scenarios declaratively.

## Coverage targets

- Rust core: 80%+ line coverage on `planner`, `applier`, `rule_sync`, `workspace_inventory`
- UI: 60%+ on components with non-trivial logic; smoke coverage on layout components
- Integration: every IPC command has at least one happy-path and one error-path test

Measure via `cargo tarpaulin` (Rust) and Vitest's built-in coverage (UI).

## CI matrix

GitHub Actions or equivalent:

| Job | OS | Rust | Node | What |
|-----|-----|------|------|------|
| `rust-test` | ubuntu-latest, macos-latest | stable | n/a | `cargo test --workspace` |
| `ui-test` | ubuntu-latest | n/a | 20 | `pnpm test` |
| `lint` | ubuntu-latest | stable | 20 | `cargo clippy`, `pnpm lint`, `cargo fmt --check`, `pnpm format:check` |
| `e2e-smoke` | macos-latest (release only) | stable | 20 | `pnpm test:e2e` |

## Performance regression tests

Defer to M4. Once the projection engine is stable, add a fixture-based benchmark suite (`criterion` for Rust) tracking:

- Cold scan of 500-item shared root
- Plan computation for a typical desired-state diff
- Apply throughput for 100 ops

Surface regressions in CI by comparing against committed baselines.

## What we do not test

- The behavior of the target tools themselves (Codex, Claude, Cursor, OpenClaw). We test that Agentic Hub writes the right files at the right paths; whether those tools then load them correctly is their concern.
- Real network — there are no network calls in v1
- macOS / Linux GUI rendering pixel-by-pixel — design QA is manual

## Open questions

- Should we publish coverage reports publicly? Defer; personal tool, low signal value
- Should we run E2E on every PR or only on release branches? Defer; start with release-only and revisit if regressions sneak in
- Should we add property-based tests (`proptest`) for the planner state machine? Decision: yes, in M4 hardening — the state-transition matrix is finite but big enough that property tests will catch missed cases
