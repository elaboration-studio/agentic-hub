# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.8.0] — 2026-06-10

### Highlights

- **A layered command palette.** Summoning the palette now lands on a hub of
  first-class commands grouped into sections — **Search** (all resources,
  skills, agents, rules, hooks, commands, suites), **Go to** (global,
  workspace), **Navigate** (Manager, Suites, Config), and **Actions** (apply a
  suite, pause/resume watching). Pick a mode first, then type: a query targets
  exactly the slice of resources you mean, instead of one global mixed result
  list. Cross-kind search is still one drill-in away via "Search all resources".
- **Go to global.** Locate any shared resource in the Manager matrix — same
  scroll-and-highlight affordance the workspace locate already had, now for the
  global scope too.
- **Toggle watching from anywhere.** The palette's Pause/Resume watching action
  flips the source watcher without surfacing the main window; the header toggle
  stays in sync.
- **Jump between search modes with Ctrl+number.** Ctrl+1…Ctrl+7 switch into
  each search slice from any palette view; the hub labels each row with `⌃1`…`⌃7`
  so the shortcuts are discoverable.

### Changes

- Typing at the palette root filters the hub commands only — resource,
  workspace, and suite results appear inside their dedicated modes.
- Every drill-in view shows a breadcrumb; Backspace on an empty query steps back
  one level (suite-tools returns to the suite search, then the root).
- The `hub-locate` event payload is now scope-tagged (`global` or `workspace`);
  a new `hub-watcher-changed` event syncs the watcher toggle across windows.

### Migration

- **None required.** No settings or on-disk format changed.

### Known Issues

- None.
