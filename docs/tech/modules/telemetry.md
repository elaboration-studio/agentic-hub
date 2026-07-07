# Module: Aptabase Telemetry

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-07
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [docs/product/telemetry-options.md](../../product/telemetry-options.md)
Related Docs: [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Anonymous, opt-in usage telemetry via Aptabase. The Rust shell sends coarse
lifecycle signals so we can tell how many desktop installs are launched and how
many stay actively used day to day. Nothing is sent while telemetry is disabled.

## Boundaries

- `agentic-hub::telemetry` owns consent gating, Aptabase event emission, and the
  daily-active engagement hook.
- `agentic-core::client_telemetry` owns the Rust-only client identity file and
  once-per-day dedupe state. The WebView never reads or writes it.
- React exposes only the on/off toggle in Config. It never calls Aptabase.

## Events

| Event | When | Notes |
| --- | --- | --- |
| `app_started` | App launch | Fires once per process start when telemetry is on |
| `app_exited` | Process exit | Flushed blocking before shutdown |
| `daily_active` | User engagement | At most once per UTC calendar day per installation |

`daily_active` includes a stable anonymous `clientId` prop (generated once per
install, stored in `~/.agentic-hub/telemetry/client-state.json`) so Aptabase can
count unique active desktop clients per day.

## Engagement signals

Background launches alone do **not** emit `daily_active`. The ping fires on the
first user interaction of the UTC day:

- Main window receives focus
- Command palette is summoned
- App is reopened from the Dock / taskbar (`RunEvent::Reopen`)

Dedupe is keyed on UTC `YYYY-MM-DD` in client state. Later interactions the same
day are ignored.

## Storage

```text
~/.agentic-hub/telemetry/client-state.json
```

| Field | Purpose |
| --- | --- |
| `installationId` | Stable anonymous id sent as `clientId` on `daily_active` |
| `lastDailyActiveDate` | UTC day of the last recorded ping |

## Consent

Mirrors `Settings.telemetry.enabled` into `TelemetryState` at startup and on
every settings save. Disabled telemetry sends nothing — including on exit flush.

## Tests

- `should_record_daily_active` dedupes within the same UTC day.
- Client state roundtrips and generates an installation id on first use.
- `utc_date_yyyy_mm_dd` matches the ISO-8601 date prefix.
