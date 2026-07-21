# Module: Skill Sources

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-21
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
- `cmd_resolve_sources() -> SourceConfig[]` — the resolved source forest (stable
  `id`s filled in), for the install window's Library-scope source picker.

Install and update run in a **dedicated `install` window** (live output +
Cancel), not blocking commands — see [The install window](#the-install-window).
Error codes: `invalid_skill_ref`, `invalid_skill_slug`, `invalid_dest_subpath`,
`unknown_provider`, `source_not_found`, `staging_failed`, `skill_search`,
`skill_cli_missing`, `install_failed`, `workspace_not_found`. See
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

One `install` window (mirroring the `palette` window) drives every explicit
skill write, streaming output live with a Cancel. It has two **scopes**
(`InstallScope`, a `Workspace | Library` toggle in install mode) and two
**modes** — install (the favorites matrix/list) or, when its mount context
carries an `update` target, update (one skill, one button). The lifecycle
(all in `crates/agentic-hub/src/install_window.rs`):

- `cmd_open_install_window(workspaceId)` — the workspace FAB stores an
  `InstallContext { workspaceId: Some(id), workspaceLabel: Some(label), update:
  None }` and builds/shows the window. The window itself renders the
  Workspace | Library toggle; switching to Library shows a source picker
  (`cmd_resolve_sources`) and a destination-subpath input instead of the
  per-tool matrix.
- `cmd_open_update_window(workspaceId, provider, installRef, name)` — a
  Workspace-scope row's "Update via skills.sh" action stores the same context
  with `update: Some(UpdateTarget { provider, installRef, name, scope:
  Workspace, .. })`.
- `cmd_open_library_update_window(provider, installRef, name, sourceId,
  destSubpath)` — a Global row's "Update via skills.sh" action (a skill a
  source root's lock manages) opens the window with no workspace at all:
  `InstallContext { workspaceId: None, workspaceLabel: None, update:
  Some(UpdateTarget { scope: Library, sourceId: Some(..), destSubpath:
  Some(..), .. }) }`.
- `cmd_take_install_context()` — the window reads its target on mount (re-read
  on the `install-context-changed` event if reopened); `update.scope` selects
  the fixed mode/scope, or install mode starts in Workspace scope with the
  toggle live.
- `cmd_install_skill_stream(input, Channel<SkillInstallEvent>)` — `input.scope`
  branches the whole command (see [Library install](#library-install) for the
  Library branch). Workspace scope is unchanged: validates the ref + slug,
  resolves the workspace dir against the target store, builds
  `skills_install_command`, and runs the shared stream helper with
  `workspace-changed` as the post-success signal.
- `cmd_update_skill_stream(input, Channel<…>)` — same `scope` branch. Workspace
  scope still delegates to `skills_update_command(name)`. Library scope has no
  CLI counterpart once a skill is normalized into the contract layout (it may
  have moved/nested under `skills/`), so it re-runs the **same staged install**
  for the recorded `installRef`/`slug` and overwrites the same destination —
  the Hub-owned equivalent of `skills update`.
- The shared `run_skill_stream` helper spawns the command with piped stdio,
  stores the `Child` in `InstallState`, and streams one `line` event per output
  line, returning `(ok, cancelled)` once the process exits — it owns none of
  the scope-specific post-processing or the terminal `done` event; each branch
  in `cmd_install_skill_stream`/`cmd_update_skill_stream` does that itself
  (Workspace: `restart_if_running` + `workspace-changed`; Library: normalize +
  lock + `sources-changed` — see below).
- `cmd_cancel_install()` — kills the stored child; the stream then ends as
  `cancelled`. Closing the window kills any in-flight child too.

Runs are sequential, so a single in-flight child in `InstallState` is enough.
`SkillInstallEvent` is the streamed, ts-rs-exported event type.

## Library install

Workspace scope installs into **one project**, tracked in that project's own
`skills-lock.json` — throwaway, per-repo. Library scope installs into a **Hub
source root** (the user's shared agentic-resources repo, e.g. `~/.agentic-arno`)
in the [shared-root contract](../reference/shared-root-contract.md) layout
(`skills/<destSubpath>/<name>/`), so one install becomes a normal Hub
capability — projectable to every tool and project via the existing Apply flow,
and re-scanned/badged like any other source-root skill.

**Why staging is mandatory.** The scanner only walks
`skills/<...>/<name>/SKILL.md` under a source root. `npx skills add` writes
into agent-native dirs (`.claude/skills/<name>/`, etc.), which the scanner never
sees. So a library install always: stages `npx skills add <ref> --skill <slug>
--agent claude-code --copy --yes` in a scratch `tempfile::TempDir` (any fixed
agent works — the produced tree is discarded once the skill folder is copied
out; `--copy` avoids symlinks to resolve), locates the produced skill folder,
then copies it into the contract layout and records it in that source root's
own `skills-lock.json`.

Pure pieces, all in `agentic-core`:

- `skill_source::skills_library_npx_args` / `skills_library_install_command` —
  the fixed, non-interactive argv (mirrors the workspace variant minus tool
  selection, which is meaningless at install time; projection happens later).
- `skill_source::locate_installed_skill(stagingDir, name)` — walks the staged
  tree (bounded depth, skipping `node_modules`/`.git`) for a directory named
  `name` that directly contains `SKILL.md`.
- `source_skill_lock::validate_dest_subpath(sub)` — the injection/traversal
  guard for the user-supplied destination (empty, or `/`-separated
  `[A-Za-z0-9._-]` components, never absolute, never `.`/`..`) before it is
  joined onto a real path.
- `source_skill_lock::normalize_into_source_root(stagedDir, sourceRoot,
  destSubpath, name)` — copies the staged folder to
  `<sourceRoot>/skills/<destSubpath>/<name>/` (re-confirms the resolved path
  stays inside `<sourceRoot>/skills/` before touching disk), computes the
  `SKILL.md` content hash, and returns the relative `skillPath` for the lock.
- `source_skill_lock::{read_source_lock, upsert_entry, write_source_lock}` —
  the source-root lock (`<sourceRoot>/skills-lock.json`), same schema the real
  skills.sh CLI writes (`version`, `skills: { name: { source, sourceType,
  skillPath, computedHash } }`). Tolerant read (malformed → empty, matching
  `skill_lock`'s workspace-lock tolerance); atomic write with a one-level
  `.bak` backup (`paths::back_up_dotfile`), like `skill_favorites`.

The impure orchestration lives in `install_window.rs`'s `LibraryInstallCtx` +
`finish_library`: once the staged process exits, it locates the skill,
normalizes it, upserts + writes the lock, and — only on success — emits
`sources-changed` (the Global Manager's re-scan signal; there is no "active
workspace" tool dir to nudge the watcher toward, so this differs from
Workspace scope's `workspace-changed`). `ctx.staging` (the `TempDir`) is
dropped — removing the scratch directory — at the end of `finish_library`
regardless of outcome.

**Badging (the reverse lookup).** `source_skill_lock::mark_locked_library_skills(sources,
items)` matches every scanned skill item (by leaf name, within that item's own
`source_id`, so two sources can each lock a same-named skill without colliding)
against its owning source root's lock, and recovers the `destSubpath` a lock
entry's `skillPath` implies (`dest_subpath_from_skill_path`, the structural
inverse of `normalize_into_source_root`'s path-building). `api::scan` runs this
after the forest scan and returns it as `ScanResult.locked_skills`, which
`useManagerStore.refresh()` turns into the same `lockedSkills` map the
Workspace path already used for the "skills.sh" badge + "Update via skills.sh"
row action in `Matrix.tsx` — one badge/action code path, two scopes.

## Testing

- `skill_source`: `validate_install_ref` accept/reject (metachars, traversal,
  empty components), fixed `npx` arg vector, `skills_install_command` program +
  argv, install rejects a bad ref before spawning, provider lookup, `search_url`
  keyless+encoded, `parse_search_response` link enrichment (GitHub vs well-known) +
  malformed JSON, short-query short-circuit, the `update` argv + command
  (`skills_update_npx_args` / `skills_update_command`), the library argv +
  command (`skills_library_npx_args` / `skills_library_install_command`), and
  `locate_installed_skill` (nested match, requires `SKILL.md` directly inside,
  absent → `None`, skips `node_modules`/`.git`).
- `source_skill_lock`: source-root lock read (missing → empty default,
  malformed → typed error, real-shape parse), upsert + write + roundtrip +
  backup-on-rewrite, `validate_dest_subpath` accept/reject,
  `normalize_into_source_root` (nested dest, empty dest, update-replaces-prior,
  rejects path-traversal dest, rejects unsafe name), `dest_subpath_from_skill_path`
  (nested/flat recovery, fallback to empty on an unexpected shape), and
  `mark_locked_library_skills` (matches within the owning source only, tolerant
  of missing/malformed locks).
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
- Manual (needs Node/npx + network, not run in CI): install one real starred
  skill into a scratch source root via Library scope; verify the contract
  layout, lock entry, no leftover `.claude/` staging residue, the Global badge,
  Apply-to-tool, then Update via skills.sh.

## Follow-ups

- Additional `SkillProvider` implementations for other registries.
- Bulk/multi-skill library install in one run (today: sequential single
  installs, same as workspace scope).
- Removing a library-installed skill via the Hub (today: filesystem-only).
