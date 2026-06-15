# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.8.2] — 2026-06-15

### Highlights

- **Opt-in usage telemetry.** A new Config ▸ Usage telemetry toggle (off by
  default) lets you share anonymous lifecycle events — only `app_started` and
  `app_exited`, plus your OS and app version — via Aptabase to help improve the
  tool. Events are sent from Rust only and gated on your consent at runtime: the
  WebView never calls out, and nothing leaves your machine while the toggle is
  off. No file contents, paths, or personal data are ever sent.
- **Refreshed app logo and bundled icons** across the desktop, iOS, and Android
  icon sets.

### Fixed

- **Startup panic from the telemetry plugin.** The app no longer panics at
  launch ("there is no reactor running") — it now owns a Tokio runtime so the
  telemetry plugin's background flush loop can start cleanly.

### Migration

- **Automatic.** No file formats changed and no manual steps are needed.
  Telemetry stays off until you turn it on.
