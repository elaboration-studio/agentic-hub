# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.6.0] — 2026-06-04

### Highlights

- **Install skills from skills.sh — no API key (opt-in).** Enable the source in
  Config, open the new **Resources** tab, and search
  [skills.sh](https://skills.sh) directly. Search uses the keyless public index
  (the same endpoint the `skills` CLI uses), so anyone can use it. **Star** the
  skills you want; from Workspace scope, **Install skill…** runs the source CLI
  into the active project and the read-only inventory re-scans to show what
  landed. Built behind a provider seam, so more public resource channels can plug
  in later.
- **The "Skills" tab is now "Resources."** skills.sh is the first of several
  planned channels for installing agentic resources, and the tab name reflects
  that broader scope.
- **External links now open.** The "Open on skills.sh" and "Open on GitHub"
  buttons on each result open in your default browser again.

### Changes

- Config gains a skills.sh panel (enable toggle, starred-file override, CLI
  check). The **Resources** tab (shown only when enabled) searches skills.sh and
  stars favorites to `~/.agentic-hub/skills-favorites.json`. Search needs **no
  API key** — it uses the keyless public index (`https://skills.sh/api/search`),
  routed through Rust (`cmd_search_skills`) since that endpoint sends no CORS
  header.
- Workspace scope gains an **Install skill…** action backed by
  `cmd_install_skill`, which runs `npx skills add <owner/repo>` via a controlled
  subprocess (no `tauri-plugin-shell`) into the active project and re-scans the
  read-only inventory. New `agentic-core` modules `skill_source` (provider seam +
  `SkillsShProvider`) and `skill_favorites`, plus `SkillsConfig` on `Settings`.
- External links route through a new `cmd_open_url` command. Anchor navigation
  is a no-op inside the WebView, so the UI now asks Rust to open `http`/`https`
  URLs in the system browser after validating the scheme and host
  (`agentic-core::open_targets::is_safe_external_url`).

### Migration

- None. The skills.sh source is opt-in and off by default; existing settings and
  workspace state load unchanged.
