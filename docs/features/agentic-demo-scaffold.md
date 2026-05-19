# Feature: Agentic Demo Scaffold

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [PRODUCT.md](../../PRODUCT.md), [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md)
Related Docs: [docs/tech/modules/agentic-demo-scaffold.md](../tech/modules/agentic-demo-scaffold.md), [docs/tech/reference/shared-root-contract.md](../tech/reference/shared-root-contract.md)

## Why now

A first-time user installs Agentic Hub and points it at `~/.agentic`. The directory does not exist. The app shows an empty state with a "Scaffold Demo Resources" button. One click later, the user has a working starter tree they can enable into tools immediately, without copying a personal repo or hand-creating a directory layout.

Without this affordance, the first-run experience is "read the docs, create a directory structure, place files, restart". With it, the first-run is "click, see, apply".

## User story

As a first-time user installing Agentic Hub on a fresh machine, I want a one-shot command that materializes a sane starter shared root with example skills, agents, and rules, so that I can use the app immediately and learn the contract by example.

## Scope

### In scope

- Tauri command `cmd_scaffold_demo({ mode })` that materializes a bundled template tree at the configured `sharedRoot`
- Two modes:
  - **merge** — write only missing paths (safe reruns; never overwrite user-added files)
  - **overwrite** — replace existing files with bundled template copies (but do not delete user-added files)
- UI surface: empty-state button when the shared root is missing or empty; settings-menu option when it exists
- Bundled content embedded in the Tauri app resources (no network fetch)
- Result summary: written / skipped / errors counts

### Out of scope

- Automatic enable-into-tools after scaffold (separate explicit user action)
- Migration from `~/.agentic-arno` or other existing shared roots
- Customizable templates (only the bundled one in v1)
- npm / pnpm tooling inside the scaffold for skill locking (referenced in bundled `ONBOARD.md` but not validated by the app)

## Bundled content

The bundled tree lives in `resources/agentic-demo/` in the repo and is embedded into the Tauri app at build time. Contents:

```
agentic-demo/
  README.md              # what is this directory
  ONBOARD.md             # how to use it with Agentic Hub
  package.json           # optional skill-lock tooling
  scripts/
    skills-manage.mjs    # npx-style skill management workflow
  .skill-lock.json       # empty lock file
  skills-lock.json       # empty lock file (alternate name)
  skills/
    agentic-hub-setup/
      SKILL.md           # tutorial skill that teaches the app's concepts
    npx-skills-workflow/
      SKILL.md           # tutorial skill that explains the optional lock workflow
  agents/
    agent-resources-manager.md   # demo agent file
  rules/
    general/
      workspace.mdc      # generic workspace conventions rule
```

The skills, agent, and rule are tutorial-grade — they document the contract by being readable examples.

## UI affordance

### Empty state (shared root missing or empty)

When `cmd_inspect` returns zero items and the shared root does not exist or is empty:

```
+----------------------------------------------------------+
|             No capabilities found at                     |
|             ~/.agentic                                   |
|                                                          |
|     [Scaffold Demo Resources]   [Choose Different Root]  |
+----------------------------------------------------------+
```

Clicking "Scaffold Demo Resources" runs `cmd_scaffold_demo({ mode: 'merge' })` and re-scans on completion.

### Settings menu

A "Scaffold Demo Resources…" option in the Settings menu opens a dialog:

- Mode: `merge` (default, safe) / `overwrite` (replace existing)
- Confirmation: shows the target dir and the bundled file count
- Run button

After the run, the result summary toast reports counts.

## Acceptance criteria

- [ ] Empty-state button appears when shared root does not exist or contains no scanned items
- [ ] Running scaffold in `merge` mode at an existing populated dir does not overwrite any existing file
- [ ] Running scaffold in `overwrite` mode at an existing populated dir replaces bundled template files but does not delete user-added files
- [ ] After scaffold, the shared root contains `skills/`, `agents/`, `rules/`, plus `README.md`, `ONBOARD.md`, the bundled skill / agent / rule examples
- [ ] After scaffold, a re-scan finds the bundled skills, agents, and rules
- [ ] Result summary reports written count, skipped count (existing files left alone), and errors count
- [ ] Scaffolding to a path that is a file (not a dir) is refused with a clear error
- [ ] Bundled content is embedded in the binary — no network fetch, no missing-file errors after install
- [ ] An explicit user action (Apply in the main window) is required to enable items into tools; scaffold itself never writes outside the shared root

## Dependencies

- `agentic-core::settings` for resolving `sharedRoot`
- New `agentic-core::scaffold` module
- Bundled `resources/agentic-demo/` embedded via `include_dir!` or Tauri resources
- New IPC command: `cmd_scaffold_demo({ mode })` (see [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md))

## Behavior detail

### Merge mode

```
for each bundled file:
    target = <shared_root> / <relative_path>
    if target.exists():
        skipped += 1
    else:
        ensure_parent_exists(target)
        write_bundled_content(target)
        written += 1
```

### Overwrite mode

```
for each bundled file:
    target = <shared_root> / <relative_path>
    ensure_parent_exists(target)
    write_bundled_content(target)  # atomic: .tmp + rename
    if existed: replaced += 1 else: written += 1
```

User-added files outside the bundled set are untouched in both modes.

### Path safety

- `sharedRoot` is canonicalized before any write
- Bundled relative paths are sanitized (no `..`, no absolute paths)
- If `sharedRoot` is a file (not a directory), refuse with `ErrPathIsFile`
- If `sharedRoot` does not exist, `mkdir -p` it first

## Risks and edge cases

- **Shared root inside a tracked git repo** — bundled files appear as untracked. Documented in `ONBOARD.md`; not a problem for the app
- **Permission denied** — surface error; partial scaffold tolerated (some files written, others not)
- **User has a custom `package.json` at the shared root** — `merge` mode preserves it; `overwrite` mode replaces with the bundled `package.json`. Confirmation copy warns explicitly about this.
- **Disk full** — partial scaffold; errors reported; user can retry

## Metrics or signals

- Adoption of scaffold (number of users who click it vs configure manually)
- Repeat scaffolds (signals exploration of the bundled examples)

## Open questions

- Should we update the bundled tree over time as the app's contract evolves? Decision: yes; each release embeds the current bundle. A user running `overwrite` after upgrading gets the latest examples.
- Should we add a "validate shared root structure" command separate from scaffold? Defer; the scan errors already surface invalid structure.
- Should the bundled rule have a placeholder for user customization? Decision: keep it minimal; users edit after scaffold.
