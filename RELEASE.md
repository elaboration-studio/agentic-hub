# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.6.1] — 2026-06-04

### Highlights

- **One unified scope view.** The Global / Workspace dropdown is gone. The
  Manager now has a single left rail with **Global pinned on top** and your
  remembered project folders below — click to switch. The rail stays pinned and
  scrolls on its own, so picking a project never means scrolling past a long
  inventory to get back to the list.
- **Filters reset when you switch scope.** Filters that only make sense in one
  scope (a source, "enabled only", collapsed folders) no longer follow you into
  the other and hide everything; your search, type, and view carry over.
- **A dedicated, live install window.** Installing starred skills now opens its
  own window: the **skill × tool matrix** (tick any skills and tools, with a
  column header to select a tool across all skills), a **live console** that
  streams the `npx skills add` output as it runs, and a **Cancel** button that
  stops a running install. The round **+** button in workspace scope opens it;
  one failed install never aborts the rest.

### Fixed

- **Installs that failed with "No such file or directory (os error 2)" now
  work.** The hub read your `PATH` from a shell that skipped `.zshrc`, so a
  Node.js installed via nvm/fnm/Homebrew was invisible and `npx` couldn't be
  found. It now reads `PATH` from your interactive login shell, so installs find
  Node the same way your terminal does — and a missing `npx` shows a clear,
  actionable hint instead of an opaque error.
- **A round of Manager polish.** The top toolbar now lines up with the matrix
  below it, the flat / tree view buttons clearly show which layout is active,
  and skill search moved into a focused modal so your starred skills get the
  full page.

### Migration

- None. Settings, favorites, and workspace state load unchanged. The scope
  dropdown is replaced by the rail; nothing to reconfigure.
