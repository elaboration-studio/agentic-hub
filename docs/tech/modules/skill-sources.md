# Module: Skill Sources

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.workspace.md](../../../ARCHITECTURE.workspace.md)
Related Docs: [docs/features/skills-sh-integration.md](../../features/skills-sh-integration.md), [docs/tech/modules/workspace-inventory.md](./workspace-inventory.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Specify the pluggable public **skill source** layer: how the hub searches and
stars skills from an external registry (skills.sh today), installs one into a
workspace via a controlled subprocess, and **updates** an already-installed one.
Install and update are the only two workspace write paths; both are explicit and
user-initiated. The inventory scan stays read-only.

## Provider seam

`agentic-core::skill_source` defines a small trait so future registries plug in
without touching the command layer:

```rust
pub trait SkillProvider {
    fn id(&self) -> &'static str;
    fn cli_check(&self) -> SkillCliStatus;
    fn install(&self, workspace_dir: &Path, install_ref: &str,
               skill: Option<&str>, tools: &[ToolId])
        -> Result<SkillInstallResult>;
}

pub fn provider_for(id: &str) -> Option<Box<dyn SkillProvider>>; // "skills.sh" today
```

- `SkillCliStatus { available, version, message }` — advisory health, surfaced in Config.
- `SkillInstallResult { ok, log }` — the blocking trait's result (child exit
  status + combined output). The live `install` window streams instead, calling
  `skills_install_command` directly; either way the authoritative view of what
  landed is the subsequent re-scan, not this struct.

### SkillsShProvider

Installs via `npx skills add <owner/repo>`. Pure, unit-tested bits:

- `validate_install_ref(ref)` — accepts only `owner/repo` (optionally
  `owner/repo/skill`): `[A-Za-z0-9._-]` per component, ≥1 `/`, no empty / `.` /
  `..` components. This is the injection guard before the ref reaches a process
  arg — it can never carry shell metacharacters or path traversal.
- `validate_skill_slug(slug)` — non-empty, only `[A-Za-z0-9._-]`, never leading
  with `-` (so it can't be read as a flag); the slug guard before it becomes an arg.
- `skills_npx_args(ref, skill, tools)` →
  `["--yes", "skills@latest", "add", ref, "--skill", slug, "--agent", id, …, "--yes"]`.
  The invocation is **fully non-interactive**: `--skill <slug>` pins the one
  starred skill (a multi-skill repo otherwise opens an interactive picker that
  hangs a headless run), one `--agent <id>` per selected tool targets exactly
  those agents (else the CLI prompts for agents), and the trailing `--yes` skips
  the skills CLI's own confirmation. Tools map to the CLI's agent ids — `claude`
  → `claude-code`; `codex` / `cursor` / `openclaw` match. `--skill`/`--agent` are
  omitted when unspecified (whole-repo, auto-detected agents).
- `skills_install_command(ref, skill, tools)` → a ready-to-spawn `Command`
  (program `npx`, the args above, login `PATH` applied, `DISABLE_TELEMETRY=1`).
  Pure assembly, no spawn, so the exact program + argv is asserted in tests and
  the shell layer owns IO (sets `cwd`, pipes stdio, spawns, streams).
- `skills_update_npx_args(name)` →
  `["--yes", "skills@latest", "update", name, "--project", "--yes"]`, and
  `skills_update_command(name)` → the matching ready-to-spawn `Command`. Updates
  one already-installed skill **by its lock name** (the `skills-lock.json` key,
  re-validated with `validate_skill_slug`); `--project` pins project scope (the
  skill lives in the project lock, not the global one) and `--yes` skips the
  scope prompt. Same login `PATH` + telemetry handling as install.

The spawn is thin: the ref is passed as an **argument vector element** (never
interpolated into a shell string), with `cwd = workspace_dir` and
`DISABLE_TELEMETRY=1`.

**macOS PATH gotcha.** A Dock-launched app inherits a minimal `PATH` that omits
Homebrew / nvm / fnm, so `npx` is often invisible. The provider reads the user's
shell `PATH` once and applies it to the child's environment. It asks an
**interactive login** shell — the user's `$SHELL` first, then `/bin/zsh`,
`/bin/bash`, `/bin/sh` — via `-ilc 'printf "<marker>%s<marker>" "$PATH"'`. The
`-i` matters: a plain login shell (`-lc`) sources `.zprofile`/`.zlogin` but
**skips `.zshrc`**, which is exactly where version managers and Homebrew usually
export `PATH`; without it `npx` resolves to "No such file or directory (os error
2)". The output is framed by a sentinel so rc-file banner chatter can't corrupt
the captured `PATH`. The shell is used **only** to read `PATH`; the install
command itself is a fixed arg vector, so no untrusted string ever reaches a
shell. If the spawn still fails with `NotFound`, the provider returns a typed
`SkillCliMissing` error with an actionable hint instead of the raw OS error.

## Local favorites store

`agentic-core::skill_favorites` persists starred skills at
`~/.agentic-hub/skills-favorites.json` (override via
`settings.skills.favorites_path`). skills.sh has no favorites API, so this is a
local-only reference list reused across projects.

```rust
pub struct SkillFavorite {
    pub provider: String,    // "skills.sh"
    pub id: String,          // provider-scoped stable id ("{source}/{slug}")
    pub slug: String,
    pub name: String,
    pub source: String,      // owner/repo
    pub install_ref: String, // what the installer receives (owner/repo)
    pub github_url: Option<String>,
    pub page_url: Option<String>,
    pub starred_at: String,  // stamped by the store on add
}
pub struct SkillFavoritesState { pub favorites: Vec<SkillFavorite> }
```

API: `read` / `add` (upsert by `(provider, id)`, newest-first, stamps
`starred_at`) / `remove`. Atomic tmp+rename write preceded by a one-level
`<file>.bak` backup of the prior good (non-empty) file
(`paths::back_up_dotfile`); missing file → empty; malformed JSON → `StateParse`
(never silently overwrites the user's file).

Because the favorites file is meant to be git-synced across devices (set a custom
`favorites_path` inside a repo), it gets the same robustness as the suites file:
the watcher subscribes to the resolved path and emits `skills-favorites-changed`
on an external rewrite, so the Resources view reloads after a `git pull` instead
of holding a stale list (see [watcher.md](./watcher.md)).

## Settings

`settings.skills: SkillsConfig { enabled, favorites_path }`, off by
default (`#[serde(default)]`, so pre-existing `config.json` files load
unchanged — any legacy `apiKey` key is ignored).
`Settings::resolved_favorites_path()` mirrors `resolved_suites_path()`. No API
key: search uses the keyless public index.

## Search (Rust)

`SkillProvider::search` runs in the Rust core (`skill_source.rs`) via a blocking
`reqwest` GET against the keyless public index
`https://skills.sh/api/search?q=…&limit=…` — the same endpoint the `skills` CLI
uses, **not** the key-gated `/api/v1/*` surface. It runs in Rust rather than the
WebView because that endpoint sends no CORS header. `search_url` and
`parse_search_response` are pure and unit-tested; the GET is the only impure
part. Queries under two characters return `[]` without a request. Links
(`installRef`, `githubUrl`, `pageUrl`) are derived once in Rust so the UI never
reconstructs URLs.

## IPC surface

- `cmd_skill_cli_check({ provider }) -> SkillCliStatus`
- `cmd_search_skills({ provider, query, limit? }) -> SkillSearchHit[]`
- `cmd_list_skill_favorites() -> SkillFavoritesState`
- `cmd_add_skill_favorite(favorite) -> SkillFavorite`
- `cmd_remove_skill_favorite({ provider, id })`

Install and update run in a **dedicated `install` window** (live output +
Cancel), not blocking commands — see [The install window](#the-install-window).
Error codes: `invalid_skill_ref`, `invalid_skill_slug`, `unknown_provider`,
`skill_search`, `skill_cli_missing`, `install_failed`, `workspace_not_found`. See
[tauri-ipc-contract.md](./tauri-ipc-contract.md).

## The project lock (`skill_lock`)

`agentic-core::skill_lock` reads the skills.sh project lock at
`<workspace>/skills-lock.json`: `LocalSkillLock { version, skills: BTreeMap<name,
LockedSkillEntry { source, source_type }> }`. `parse_local_lock(body)` is pure
and unit-tested; `read_local_lock(ws)` is **tolerant** — a missing file or
malformed JSON yields `None`, so a third-party lock can never break the read-only
scan. The workspace inventory uses it to mark which skill rows the CLI manages
(see [workspace-inventory.md](./workspace-inventory.md)); the lock — not any
hub-kept state — is the project-local truth.

## The install window

The two explicit workspace writes are driven from one `install` window
(mirroring the `palette` window) so output streams live and the run is
cancellable. The window runs in **install mode** (the favorites matrix) or, when
its mount context carries an `update` target, **update mode** (one skill, one
Update button). The lifecycle (all in `crates/agentic-hub/src/install_window.rs`):

- `cmd_open_install_window(workspaceId)` — the workspace FAB stores an
  `InstallContext { workspaceId, workspaceLabel, update: None }` and builds/shows
  the window.
- `cmd_open_update_window(workspaceId, provider, installRef, name)` — a row's
  "Update via skills.sh" action stores the same context with
  `update: Some(UpdateTarget { provider, installRef, name })`, reusing the same
  build/show path.
- `cmd_take_install_context()` — the window reads its target on mount (re-read on
  the `install-context-changed` event if reopened); `update` selects the mode.
- `cmd_install_skill_stream(input, Channel<SkillInstallEvent>)` — validates the
  ref + slug + resolves the workspace dir against the target store (installs only
  into a remembered target; `input` carries the `slug` and selected `toolIds` so
  the spawn is non-interactive), then runs the shared stream helper.
- `cmd_update_skill_stream({ provider, workspaceId, name }, Channel<…>)` —
  validates the provider + slug + resolves the workspace dir, builds
  `skills_update_command(name)`, and runs the **same** shared stream helper.
- The shared helper spawns the command with piped stdio, stores the `Child` in
  `InstallState`, streams one `line` event per output line then a terminal
  `done { ok, cancelled }`, and on success `restart_if_running`s the watcher and
  emits `workspace-changed` so the read-only inventory re-scans.
- `cmd_cancel_install()` — kills the stored child; the stream then ends as
  `cancelled`. Closing the window kills any in-flight child too.

Runs are sequential, so a single in-flight child in `InstallState` is enough.
`SkillInstallEvent` is the streamed, ts-rs-exported event type.

## Testing

- `skill_source`: `validate_install_ref` accept/reject (metachars, traversal,
  empty components), fixed `npx` arg vector, `skills_install_command` program +
  argv, install rejects a bad ref before spawning, provider lookup, `search_url`
  keyless+encoded, `parse_search_response` link enrichment (GitHub vs well-known) +
  malformed JSON, short-query short-circuit, and the `update` argv + command
  (`skills_update_npx_args` / `skills_update_command`).
- `skill_lock`: `parse_local_lock` (real shape with ignored extra fields, empty,
  malformed), `read_local_lock` (missing / malformed → `None`, present → parsed).
- `skill_favorites`: add stamps + newest-first, upsert by `(provider, id)`,
  distinct providers, remove only the match, disk roundtrip, empty on missing.
- `settings`: skills block defaults off, roundtrips, legacy config without the
  block defaults off.
- UI store (`src/state/skills.ts`): search (success / blank / error), favorites
  load / star / unstar, and the pure `filterFavorites` helper (name / repo /
  owner / slug, case-insensitive, blank-query passthrough) that backs the local
  filter box on the Skills page and the install window. Mocks `@/ipc` + toasts.
  (Install is window-local state in `InstallWindow`, no longer in this store.)

## Follow-ups

- Additional `SkillProvider` implementations for other registries.
