# Feature: Install skills.sh Skills into the Global Library (Source Root)

Status: Shipped
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-21
Depends On: [PRODUCT.md](../../PRODUCT.md), [docs/features/skills-sh-integration.md](./skills-sh-integration.md), [docs/tech/modules/skill-sources.md](../tech/modules/skill-sources.md), [docs/tech/modules/multi-source-roots.md](../tech/modules/multi-source-roots.md), [docs/tech/reference/shared-root-contract.md](../tech/reference/shared-root-contract.md)

## Shipped shape

Implemented as planned, with these naming differences from the original draft
below (kept for the slice-by-slice rationale): the update-window open path for
a Global-scope lock is a separate command, `cmd_open_library_update_window`
(not a scope param on `cmd_open_update_window`), since it has no workspace at
all; the source-root lock module is `agentic_core::source_skill_lock`; the
badge type is `LibraryLockedSkill`; the source picker is backed by
`cmd_resolve_sources`. Full detail in
[skill-sources.md](../tech/modules/skill-sources.md#library-install).

**Entry point correction.** The install FAB only ever rendered in Workspace
scope (`ManagerView.tsx`), so "toggle to Library inside the window" was
unreachable from Global scope — there was no way to open the window at all
without a workspace open. Fixed by rendering the FAB in Global scope too, wired
to a new no-workspace open path, `cmd_open_library_install_window` /
`openLibraryInstallWindow()`, which opens the window with `workspaceId: null`
and the window defaults straight to Library scope (Workspace option disabled).

## Why now

Today the skills.sh integration installs a starred skill into **one workspace
project** via `npx skills add` (cwd = project), tracked in that project's
`skills-lock.json`. That is per-project and throwaway.

The user actually curates open-source skills in a **shared agentic resources
repo** used as a Hub *source root* (e.g. `~/.agentic-arno`, an ordered `sources`
entry in `~/.agentic-hub/config.json`). Skills placed there become first-class
Hub capabilities the projection engine can push into every tool and every
project. The user currently does this **by hand**: run the CLI somewhere, then
move the skill into `skills/<category>/<name>/` and hand-maintain
`~/.agentic-arno/skills-lock.json`. Moving the skill also breaks
`npx skills update <name>`, so updates are manual too.

This feature makes "install a skills.sh skill into the global library, and keep
it updatable" a first-class Hub action — the library counterpart to the existing
workspace install.

## The core constraint (why a normalize step is mandatory)

- The Hub scanner (`scanner::scan_all`) walks only `skills/`, `agents/`,
  `rules/`, `hooks/`, `commands/` under a source root, and a skill is a folder
  containing `SKILL.md` (see [shared-root-contract](../tech/reference/shared-root-contract.md)).
- `npx skills add` installs into **agent-native** dirs (`.claude/skills/<name>/`,
  etc.) plus a `skills-lock.json`, using `--copy` for real files. It has no mode
  that writes directly into a plain `skills/<name>/` contract layout.
- Therefore any library install must: run the CLI in a **staging dir**, locate
  the produced skill folder, and **copy it into the contract layout** at
  `<sourceRoot>/skills/<destSubpath>/<name>/`.

## Confirmed design decisions

1. **Destination**: user chooses a subfolder per install (a destination-path
   field, default `skills/`), so it matches the existing category nesting
   (`skills/arno/cmo/<name>`). Lock records the resolved relative `skillPath`.
2. **Update model**: **Hub-owned re-install + hash compare**. Update re-runs the
   staging install for the recorded `installRef`/`slug`, replaces the folder
   in-place, and updates the source-root lock hash. Independent of the CLI's
   project-scope `update` (which breaks on nesting).
3. **Entry point**: **reuse the existing Install window** with a
   `Workspace | Library` scope toggle. Library mode swaps the tool-column matrix
   for a source-root picker + destination field (tools are irrelevant at install
   time; projection to tools happens later in the Manager).

## What it does

- Adds a **Library (source root)** install target to the streaming Install
  window, alongside the existing Workspace target.
- Installs a starred skill into `<sourceRoot>/skills/<destSubpath>/<name>/` in the
  shared-root contract layout, via a staged `npx skills add` + normalize.
- Records provenance in `<sourceRoot>/skills-lock.json` (existing schema:
  `{ name: { source, sourceType, skillPath, computedHash } }`) so the skill is
  Hub-tracked and updatable.
- Marks library-installed skills with a `skills.sh` badge in the **Global**
  Manager (matched from the source-root lock) and offers **Update via skills.sh**
  on the row menu (Hub-owned re-install).
- After install/update, triggers a source rescan so the skill appears as a normal
  capability, projectable to any tool / workspace via the existing Apply flow.

## Scope

- In scope: library (source-root) install + update of a single starred skill;
  scope toggle in the Install window; source-root lock read/write; Global Manager
  badge + update action; source rescan after write.
- Out of scope (v1): bulk/multi-skill library install in one run (sequential
  single installs are fine); removing a library skill via the Hub (delete stays
  manual / filesystem); syncing the whole `skills-lock.json` the CLI already
  wrote by hand into the Hub-managed lock; auto-update on a schedule; tool
  selection during library install (projection is a separate, existing step).

## Acceptance criteria

- With `skills.enabled = true`, the Install window offers a `Workspace | Library`
  toggle. Library mode shows a source-root picker (from `settings.sources`) and a
  destination-path field defaulting to `skills/`.
- Installing a starred skill in Library mode lands
  `<sourceRoot>/skills/<destSubpath>/<name>/SKILL.md` (+ its `references/`,
  `scripts/`, etc.) and upserts a `<sourceRoot>/skills-lock.json` entry with
  `source`, `sourceType`, `skillPath` (relative to the source root), and
  `computedHash`.
- No agent-native dirs (`.claude/`, etc.) are left in the source root; staging is
  cleaned up on both success and failure.
- The install ref and slug pass the existing `validate_install_ref` /
  `validate_skill_slug` guards; the destination subpath passes a new
  path-safety guard (no absolute paths, no `..`, no leading `/`, allowed chars
  only) and always resolves inside `<sourceRoot>/skills/`.
- After a successful install, the Global Manager shows the new skill as a normal
  capability with a `skills.sh` badge, and it can be enabled + applied to tools.
- **Update via skills.sh** on a badged Global row re-installs the recorded
  ref/slug into the same destination, updates the lock `computedHash`, and the
  streaming console + Cancel behave exactly as workspace install/update.
- Existing workspace install/update behavior is unchanged.
- The WebView never receives a raw path or shell string; all spawns are fixed
  argv vectors with cwd = a Hub-resolved staging dir.

## Implementation plan (slices)

Branch: `feat/skills-sh-library-install-20260721`.

### Slice 0 — Docs first (docs-designer)
- Update [skill-sources.md](../tech/modules/skill-sources.md): add the
  **library install** path (staging + normalize + source-root lock + Hub-owned
  update) next to the workspace path.
- Update [skills-sh-integration.md](./skills-sh-integration.md): note the new
  Library target and the Global-view badge/update.
- Keep this feature doc as the working contract.
- Verify: docs cross-link; no contradiction with the read-only-inventory
  contract (workspace stays read-only; library writes only into the source root
  the user configured).

### Slice 1 — Core: staged install + normalize + source-root lock (agentic-core)
Riskiest piece; prove it first. All pure/unit-testable except the spawn.
- `skill_source.rs`:
  - `skills_library_npx_args(install_ref, slug)` → `["--yes","skills@latest",
    "add", ref, "--skill", slug, "--agent", "claude-code", "--copy", "--yes"]`
    (pure; asserted in tests). `--copy` gives real files; `--agent claude-code`
    avoids agent detection and yields a plain skill folder.
  - `skills_library_install_command(install_ref, slug)` → ready-to-spawn
    `Command` (login `PATH`, `DISABLE_TELEMETRY=1`); cwd is set by the caller to
    the staging dir. Mirrors `skills_install_command`.
  - `locate_installed_skill(staging_dir, slug) -> Option<PathBuf>` — find the
    produced `<staging>/.claude/skills/<name>/` folder (pure over a walked dir;
    unit-tested against a fixture tree).
- New `source_skill_lock.rs` (or extend `skill_lock.rs`):
  - `read_source_lock(source_root) -> SourceSkillLock` (missing → empty;
    malformed → typed `StateParse`, never overwrite).
  - `upsert_entry(&mut lock, name, entry)` and `write_source_lock(source_root,
    &lock)` — atomic tmp+rename with one-level `.bak` (reuse
    `paths::back_up_dotfile`). Schema matches the existing
    `skills-lock.json` (`version`, `skills: { name: { source, sourceType,
    skillPath, computedHash } }`).
  - `validate_dest_subpath(sub) -> bool` — rejects absolute, `..`, leading `/`,
    disallowed chars; used to build the safe destination.
  - `normalize_into_source_root(staging_skill_dir, source_root, dest_subpath,
    name) -> InstalledSkill` — copy the folder to
    `<sourceRoot>/skills/<dest>/<name>/`, compute `computedHash` (reuse the
    `SKILL.md`/folder hash from `managed_copy::content_hash`), return the
    relative `skillPath`.
- Verify: `cargo test -p agentic-core` — new unit tests for args, locate,
  dest-subpath guard, lock upsert/merge/round-trip, and normalize (copy +
  hash + skillPath) against a temp fixture. No network.

### Slice 2 — Command + streaming (agentic-hub)
- `install_window.rs`:
  - Extend `InstallContext` with a `target` discriminant:
    `InstallTarget::Workspace { id, label }` | `InstallTarget::Library`
    (library mode carries no fixed source; the window picks one). Keep
    `update: Option<UpdateTarget>`; `UpdateTarget` gains an optional
    source-root id + dest so update knows where to re-install.
  - `cmd_open_install_window` gains the library variant (or a new
    `cmd_open_library_install_window`) that stashes a library `InstallContext`.
  - Extend `InstallSkillInput` with `scope: InstallScope` (`Workspace` |
    `Library`), `source_id: Option<String>`, `dest_subpath: Option<String>`.
    In `cmd_install_skill_stream`, when `scope == Library`:
    - resolve the source root from `Settings::resolve_sources` by `source_id`
      (verify it exists and is a dir);
    - validate ref/slug/dest-subpath;
    - create a staging temp dir (system temp; cleaned in a guard on all exits);
    - echo `$ npx …`, spawn `skills_library_install_command` with
      cwd = staging, stream via the shared `run_skill_stream`;
    - on clean success: `locate_installed_skill` → `normalize_into_source_root`
      → `upsert_entry` + `write_source_lock`; then emit a **source rescan**
      event (not `workspace-changed`) and nudge the watcher.
  - `cmd_update_skill_stream` (library): re-run the staged install for the
    recorded ref/slug into the recorded dest, replace folder, update lock hash.
    Reuse the same streaming; do **not** call `skills update`.
- Verify: `cargo test` workspace green; `cargo clippy`. Manual: install one skill
  into a scratch source root and confirm layout + lock + no `.claude/` residue.

### Slice 3 — Frontend: scope toggle + library UI (React)
- `src/types` regenerated via `ts-export` for the new
  `InstallContext`/`InstallSkillInput`/`InstallScope`.
- `src/ipc.ts`: extend `installSkillStream`/`updateSkillStream` payloads;
  add `openLibraryInstallWindow(...)` (or a param on `openInstallWindow`);
  expose source list (reuse existing settings/sources getter).
- `InstallWindow.tsx`: add a `Workspace | Library` segmented toggle in the
  header. Library mode:
  - render a source-root `<Select>` (from `settings.sources`, remember last
    choice) and a destination-path `<Input>` (default `skills/`, live-validated),
  - replace `InstallMatrix` tool columns with a plain checkbox list of starred
    skills (pick which skills; no tool columns),
  - `onInstall` sends `scope: "library", sourceId, destSubpath` per skill,
    sequentially, reusing the existing streaming console + Cancel.
- `InstallFab.tsx` / entry: allow opening the window directly in Library scope
  (e.g. a split button or a scope arg).
- Verify: `pnpm tsc` + `pnpm test`; manual click-through of both scopes.

### Slice 4 — Global Manager badge + update action (React + core)
- Extend the source-root lock matching so the **Global** scan attaches a
  `skills.sh` source + `installRef`/`name` to items whose leaf folder matches a
  `<sourceRoot>/skills-lock.json` key (mirror `mark_locked_skills`, but over the
  source forest, keyed per source root).
- `Matrix.tsx` `RowActions`: for badged Global rows, show **Update via
  skills.sh** → open the update window in library mode for that skill
  (`sourceId`, `name`, `installRef`, `dest`).
- Verify: badge renders on the installed skill; update reopens the window,
  re-installs, and the row refreshes after rescan.

### Slice 5 — Docs sync, RELEASE, changelog, MR
- Finalize docs (Slice 0 docs reflect shipped behavior); add RELEASE entry and
  CHANGELOG note; open MR to `main`.

## Global constraints (for reviewers)

- Source-root lock file name: `skills-lock.json` at the source root; schema keys
  exactly `version`, `skills`, and per-entry `source`, `sourceType`,
  `skillPath`, `computedHash`.
- Library install command argv is fixed and includes `--copy`,
  `--agent claude-code`, `--skill <slug>`, `--yes`; ref/slug/dest validated
  before spawn; cwd is a Hub-created staging dir, never an arbitrary path.
- Staging dir is always removed (success or failure).
- Destination always resolves inside `<sourceRoot>/skills/`; reject any subpath
  that escapes it.
- Workspace scope remains read-only except for its existing install/update path;
  the library path writes only inside the user-configured source root.

## Test plan (to approve before running)

- Rust unit (fast): args builder, `locate_installed_skill`, `validate_dest_subpath`,
  source-root lock upsert/merge/round-trip, `normalize_into_source_root`
  (copy + hash + skillPath) — temp fixtures, no network.
- Frontend: type-check (`pnpm tsc`), component tests for the scope toggle and
  library payload shaping.
- Manual (opt-in, needs Node/npx + network): install one real starred skill into
  a scratch source root; verify layout, lock entry, no `.claude/` residue, badge,
  Apply-to-tool, then Update.

## RELEASE entry (draft)

- skills.sh library install: install and update open-source skills.sh skills
  directly into a Hub source root (your shared agentic-resources repo) in the
  contract layout, so one install is projectable across every tool and project
  and stays updatable from skills.sh.
