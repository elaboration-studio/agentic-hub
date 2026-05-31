---
name: empty-start-and-open-files
overview: Add a first-run empty-state that scaffolds a bundled demo shared root in one click, and add per-row "open file" actions (original via a configurable preferred editor; the tool's actually-projected/referenced file via a hidden more-actions menu) using tauri-plugin-opener through a validated Rust command.
todos:
  - id: branch
    content: Create branch feat/empty-start-and-open-files-20260601
    status: completed
  - id: demo-tree
    content: Copy VS Code agentic-demo assets into resources/agentic-demo/ at repo root
    status: completed
  - id: scaffold-core
    content: Add agentic-core::scaffold module (include_dir embed, merge/overwrite, path safety, ScaffoldResult) + unit tests
    status: completed
  - id: scaffold-cmd
    content: Add cmd_scaffold_demo command, register it, add scaffoldDemo IPC wrapper
    status: completed
  - id: empty-state-ui
    content: Add EmptyState component + styles, wire into App.tsx for zero-item global manager
    status: completed
  - id: opener-dep
    content: Add tauri-plugin-opener (Cargo + npm) and initialize the plugin
    status: completed
  - id: editor-setting
    content: Add editor preference to Settings + EditorPanel dropdown in ConfigPage
    status: completed
  - id: open-cmds
    content: Add is_openable validator + cmd_open_path / cmd_reveal_path + IPC wrappers
    status: completed
  - id: row-actions
    content: "Add per-row more-actions menu in Matrix: open original (preferred editor), reveal, open per-tool projected file"
    status: completed
  - id: types-regen
    content: Regenerate ts-rs types and update src/types/index.ts exports
    status: completed
  - id: docs-release
    content: Sync docs (scaffold feature, open-files feature, IPC contract, permissions open questions) + RELEASE entries
    status: completed
  - id: test-mr
    content: Run approved test plan, manual smoke, open MR to main
    status: completed
isProject: false
---

# Empty-Start Scaffold + Open Files

Two independent features, both already anticipated in the repo docs (`docs/features/agentic-demo-scaffold.md`, and `ARCHITECTURE.permissions.md` lines 195/223 which pre-approve a validated `cmd_open_in_editor`). Branch: `feat/empty-start-and-open-files-20260601`.

## Decisions locked
- File opening uses `tauri-plugin-opener` (official, scoped successor to the forbidden `shell` plugin), invoked only from a Rust command in the core/shell, never directly from the WebView. This honors the `AGENTS.md` hard rule.
- Preferred-editor setting is a Config dropdown: System default / VS Code / Cursor / Custom.

---

## Feature A — Empty-start one-shot scaffold

Mirrors the VS Code extension's `agenticDemoScaffoldService` + `setupAgenticDemoResources` command.

### A1. Bundle the demo tree
- Copy `e-studio-vscode-extension/packages/vs-code/assets/agentic-demo/` into a new `resources/agentic-demo/` at the agentic-hub repo root (skills, agents, rules, hooks, `README.md`, `ONBOARD.md`, etc.), matching the tree in [docs/features/agentic-demo-scaffold.md](docs/features/agentic-demo-scaffold.md).
- Rename any lingering `e-studio-*` identifiers to `agentic-hub-*` (the demo already uses `agentic-hub-setup`).

### A2. New core module `crates/agentic-core/src/scaffold.rs`
- Embed the tree with `include_dir!` (add `include_dir` dep to `agentic-core`): `static DEMO: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../resources/agentic-demo");`
- `pub enum ScaffoldMode { Merge, Overwrite }` and `pub struct ScaffoldResult { written, skipped, replaced, errors, destination_root }` (ts-export derives like other model types).
- `pub fn scaffold_demo(dest_root: &Path, mode) -> Result<ScaffoldResult>`: refuse if dest is a file (`CoreError::NotADirectory`), `create_dir_all` otherwise, walk embedded files, sanitize relative paths (no `..`/abs), merge skips existing, overwrite writes atomically (`.tmp`+rename). Per-file failure recorded, never aborts (matches the spec's behavior section).
- Register `pub mod scaffold;` in `lib.rs` and re-export the result type.
- Unit tests against a tempdir (merge skips, overwrite replaces, file-as-root refused, re-scan finds items).

### A3. IPC command
- `cmd_scaffold_demo({ mode })` in [crates/agentic-hub/src/commands.rs](crates/agentic-hub/src/commands.rs): resolve destination as `Settings::load()?.resolve_sources()[0].path` (first source / legacy `shared_root`), call `scaffold::scaffold_demo`, then `watcher.restart_if_running` so the new tree is picked up. Register in [crates/agentic-hub/src/lib.rs](crates/agentic-hub/src/lib.rs). No new capability/plugin permission (custom command, local to window).
- Add `scaffoldDemo(mode)` wrapper in [src/ipc.ts](src/ipc.ts).

### A4. Empty-state UI
- Today `App.tsx` shows only a muted "No capabilities found" banner via `Matrix`. Add a dedicated `EmptyState` component rendered in `App.tsx` when `route === "manager"`, `scope === "global"`, and `data.items.length === 0`.
- Content: headline ("No capabilities yet"), the resolved destination path, primary button "Scaffold demo resources" (runs `scaffoldDemo("merge")` then `refresh()`), secondary "Add a source…" (reuses the dialog flow already in `ConfigPage`). Show a result summary line (written/skipped) after running.
- Add minimal styles to [src/styles.css](src/styles.css) (`.empty-state`).

---

## Feature B — Open original & projected files

### B1. Add the plugin
- `crates/agentic-hub/Cargo.toml`: `tauri-plugin-opener = "2"`. `package.json`: `@tauri-apps/plugin-opener`.
- Initialize in [crates/agentic-hub/src/lib.rs](crates/agentic-hub/src/lib.rs): `.plugin(tauri_plugin_opener::init())`.

### B2. Editor preference in settings
- Add to `Settings` (`crates/agentic-core/src/settings.rs`): `#[serde(default)] pub editor: EditorPref` where `EditorPref` is an enum `{ SystemDefault, VsCode, Cursor, Custom(String) }` (serde-tagged) or a simple struct `{ kind: String, custom_app: Option<String> }`. Default = SystemDefault. Backward-compatible via `#[serde(default)]`.
- Add `editor_app_name(&self) -> Option<String>` mapping to a macOS app name (`"Visual Studio Code"`, `"Cursor"`, custom string, or `None`).
- Config UI: new `EditorPanel` in [src/ConfigPage.tsx](src/ConfigPage.tsx) — dropdown + conditional custom text field, persisted via existing `saveSettings`.

### B3. Validated open commands (core validation, shell-clean)
- Core helper in `scaffold.rs`/new `open_targets.rs` or `paths.rs`: `pub fn is_openable(path: &Path, settings: &Settings, workspace_dirs: &[PathBuf]) -> bool` — canonicalize, allow only if under a resolved source root, any configured tool path (skills/agents/rules/instructions/hooks), or a known workspace target. Matches the path-validation rule in [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md).
- `cmd_open_path({ path, openWith? })` and `cmd_reveal_path({ path })` in `commands.rs`: validate via `is_openable`, then `app.opener().open_path(path, open_with)` / `app.opener().reveal_item_in_dir(path)` (`use tauri_plugin_opener::OpenerExt`). Register both; no WebView opener capability needed since the call is Rust-side.
- IPC wrappers `openPath(path, openWith?)` and `revealPath(path)` in `ipc.ts`.

### B4. Row actions UI (more-actions menu)
- In [src/Matrix.tsx](src/Matrix.tsx) `leafRow`, add a trailing actions cell with a "More" (`⋯`) menu (small popover/details). Items:
  - "Open original" -> `cmd_open_path(originalFile, editorAppName)`. Original file = `SKILL.md`/`hook.json` inside the folder for skill/hook, else `source_path` (agent/rule). Editor app comes from settings.
  - "Reveal original in Finder" -> `cmd_reveal_path(source_path)`.
  - Per enabled tool whose `ToolCapabilityState.state === "enabled"`: "Open in <Tool>" -> `cmd_open_path(targetPath)` where `targetPath` is the state's `target_path`; for markdown-section rules (no per-item file) fall back to the adapter instruction file. Opens the file the tool actually references (symlink resolves to source; managed copy / instruction file is the tool's real copy).
- Keep the actions menu visually subtle so it doesn't crowd the toggle matrix (hidden until hover/click). Minor `styles.css` additions.

---

## Cross-cutting
- Regenerate TS types: `cargo test -p agentic-core --features ts-export` (new `ScaffoldResult`, `ScaffoldMode`, updated `Settings`); add exports in [src/types/index.ts](src/types/index.ts).
- Docs sync (`docs-designer` spirit): flip `agentic-demo-scaffold.md` status to reflect implementation; add a short open-files feature doc under `docs/features/`; add the three new commands to [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md); resolve the two open questions in [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) (opener instead of shell). Append one-line entries to `RELEASE.md`.
- Test plan (will ask before running): `cargo test --workspace`, `pnpm test`, `cargo clippy --all && pnpm lint`. Manual smoke in `pnpm tauri dev`: empty root -> scaffold -> list populates; open original opens chosen editor; more-actions opens the per-tool projected file.
- Finish with MR to `main` (no direct commits to `main`).

## Out of scope
- Auto-enabling scaffolded items into tools (explicit Apply only).
- A separate Suite Manager window (current build is single-window/route-based).
- Windows-specific `open -a` equivalents beyond what `tauri-plugin-opener` already handles.