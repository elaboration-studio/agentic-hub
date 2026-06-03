# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.3.0] — 2026-06-03

### Highlights

- Command palette: an Alfred-style floating panel summoned by a configurable
  global shortcut (default Cmd+Alt+A) that searches resources and opens the
  original file in your editor — plus native macOS menus with Cmd+, for Config.

### Changes

- New floating `palette` window toggled by a global accelerator
  (`Settings.paletteShortcut`, default `Cmd+Alt+A`), dismissed on blur or Esc.
- Flat resource search opens the original file via the existing opener allowlist;
  an extensible command-provider registry also ships navigation commands.
- Native application menu (App / Edit / View / Window); "Settings…" (Cmd+,) routes
  to Config and "Command Palette" toggles the panel. Cmd+Q stays the hard exit.
- Config gains a Command Palette panel to edit the shortcut; saving re-registers it.

### Migration

- The new `paletteShortcut` field defaults to `Cmd+Alt+A` for existing configs.
