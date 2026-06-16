# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.9.0] — 2026-06-16

### Highlights

- **Tools preflight, a new tab in the Resources panel.** Resources is now a
  two-pane view: a left rail switches between **Tools** (always available) and
  **Skills** (the skills.sh browser, shown only when that source is enabled).
  The Tools pane is a preflight check for the command-line tools agents rely on
  — Node, Python, Homebrew, git, the GitHub/GitLab CLIs, the Claude/Codex/Cursor
  agent CLIs, and Vercel. Each row shows whether the tool is installed, its
  version, and (where it applies) whether you're authenticated, with a per-row
  re-check, a "Refresh all" button, and an Install link out to each tool's site.
  It runs automatically the first time you open the tab so you can see your setup
  at a glance before starting agentic work.

### Details

- Tools come from a bundled JSON catalog (`resources/cli-tools/catalog.json`).
  Status is probed in Rust using your resolved login `PATH` (so tools installed
  via Homebrew / nvm / fnm are found even when launched from the Dock), with a
  per-check timeout; probes run each tool's `program` + `args` directly, never
  through a shell.
- An optional user-local catalog override (`cliToolsPath`) merges your own tools
  with the bundled set. Because catalog entries are executed, this path is
  **config-file-only** — it can only be set by hand-editing the settings file,
  never by the app UI — and login-`PATH` resolution is cached and
  timeout-bounded so a slow shell profile can't stall checks.

### Migration

- **None.** The Resources tab is always visible now; everything else is
  additive.

### Known Issues

- None.
