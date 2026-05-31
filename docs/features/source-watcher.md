# Feature: Source Watcher

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-31
Depends On: [docs/features/mvp-unified-agentic-capability-manager.md](./mvp-unified-agentic-capability-manager.md), [docs/features/workspace-suite-sync.md](./workspace-suite-sync.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/watcher.md](../tech/modules/watcher.md), [docs/tech/modules/multi-source-roots.md](../tech/modules/multi-source-roots.md)

## Why now

Capabilities live in source folders that are often Git repos. After a `git pull`,
an edit, or a new skill dropped beside an existing one, the projected copies in
`~/.claude`, `~/.cursor`, `~/.codex` and any active workspace go stale until the
user remembers to hit **Rescan** and re-apply. The manual button is friction and
a silent staleness trap.

The watcher closes that loop: it watches the source roots, and on any change it
re-scans, reconciles every enabled tool's projections from fresh source content,
re-patches the active workspace, and live-refreshes the UI. The Rescan button is
replaced by a single **Watch** toggle.

## User story

As a user whose shared capabilities live in a Git repo, I want the app to keep
every tool in sync automatically when the repo changes, so I never apply stale
skills, rules, or hooks — and I want one switch to pause that if I prefer manual
control.

## Scope

### In scope

- A filesystem watcher over the resolved source roots (recursive), debounced.
- On change: re-scan + **reconcile** every enabled tool and re-patch the active
  workspace, then emit `sources-changed` so the UI refreshes.
- Reconcile semantics:
  - keep + refresh already-owned projections (stale managed copies re-copied,
    broken links repaired, rule blocks + hook JSON re-synced from fresh source);
  - **auto-enable newcomers**: a brand-new capability is enabled for tool T iff a
    same-source, same-kind sibling in its immediate parent folder is already
    enabled for T (a new file in a brand-new folder stays disabled);
  - never takes over foreign files/links.
- A header **Watch / Paused** toggle (replaces Rescan), persisted in settings
  (`watcherEnabled`, default on).
- A Config **"Rescan & resync everything"** fallback button for recovery.
- Live UI refresh on change — skipped while the user has unapplied edits, so an
  incoming event never discards an in-progress selection.

### Out of scope

- Running git. The watcher reacts to filesystem events only; a `git pull` shows
  up as ordinary file writes. `tauri-plugin-shell` stays banned.
- Orphan GC of copies whose source folder was deleted (pre-existing limitation;
  a removed source just drops from the list).
- Re-patching non-active workspaces on every change (active workspace only).
- Per-source watch toggles (the watcher is all-or-nothing in v1).

## Behavior contract

- **Global scope.** For each enabled tool, owned projections are kept and
  refreshed; newcomers auto-enable by the sibling rule above.
- **Workspace scope.** Each workspace target remembers the last `(tool, suite)`
  applied to it (`WorkspaceTarget.lastApplied`). The active workspace is
  re-patched from those records so its hard copies refresh from fresh source
  content. Suites are explicit lists, so workspace content refreshes but no
  auto-enable happens there.
- **Default on.** `watcherEnabled` defaults to true; configs written before the
  flag existed default on too.
- **Loop-safe.** Reconcile is idempotent — a second pass over unchanged sources
  produces an empty plan and no writes, so writes into target dirs (which are not
  watched) cannot drive an unbounded loop.

## Experience

```
Header
  Agentic Hub
  Scope: [ Global | Workspace ]        ● Watching     (toggle)

Config ▸ Sync recovery
  [ Rescan & resync everything ]   Done — projections reconciled.
```

- The toggle shows a green live dot when watching, grey when paused. Clicking it
  calls `cmd_set_watcher_enabled`, persists the choice, and starts/stops the
  OS subscription immediately.
- On any debounced change the manager view re-scans and re-inspects itself; the
  matrix reflects refreshed states and any auto-enabled newcomers.

## Acceptance criteria

- [ ] Watcher starts on launch when `watcherEnabled` is true
- [ ] Editing a source file refreshes the matching managed copy (stale → enabled) without a manual rescan
- [ ] A broken link is repaired on the next change
- [ ] A new skill dropped beside an enabled sibling is auto-enabled; a new skill in a brand-new folder stays disabled
- [ ] Foreign files/links at a target are never taken over
- [ ] The active workspace re-patches from its recorded `lastApplied` on change
- [ ] The header toggle pauses/resumes watching and persists across restarts
- [ ] The Config fallback button forces a full rescan + resync (no auto-enable) and refreshes the UI
- [ ] A live refresh is skipped while the user has unapplied matrix edits
- [ ] Adding/removing a source or saving settings re-subscribes the watcher to current roots

## Dependencies

- `notify` (cross-platform filesystem events)
- New `agentic-core::reconcile` module (pure decision + plan/apply reuse)
- `Settings.watcherEnabled`; `WorkspaceTarget.lastApplied` (`WorkspaceApply`)
- New IPC commands: `cmd_set_watcher_enabled`, `cmd_rescan_resync`; new event
  `sources-changed` (see [tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md))

## Risks and edge cases

- **Source root set to `~`** — would watch target dirs too; debounce + idempotent
  reconcile self-limits to ~1–2 cycles. Documented as user responsibility.
- **Burst of writes (git pull)** — coalesced into one reconcile by a 400 ms quiet
  window.
- **`.git/` internal churn** — events confined to a `.git/` path are ignored.
- **Mid-edit refresh** — suppressed while pending matrix changes exist.

## Open questions

- Should the toggle live per-source once per-source enable/disable lands? Deferred.
- Surface a transient "synced N items" toast on watcher activity? Deferred — the
  matrix refresh is the signal for now.
