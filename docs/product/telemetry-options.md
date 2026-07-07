# Telemetry options comparison

> **Role of this doc.** Evaluate analytics/telemetry options for Agentic Hub so we
> can resolve the open decision **D-open-3 — Telemetry** ([decisions.md](decisions.md),
> backlog **L3**) with eyes open. This is an evaluation, not a commitment to ship.

## The question

Can a Tauri desktop app use Google Analytics (or anything like it) to track user
behavior — and *should* this app? Technically yes. The real question is which
mechanism fits a desktop WebView, and whether it's worth breaking our posture.

**Stance it must respect:** [PRODUCT.md](../../PRODUCT.md) declares "No remote
network calls in v1 (no telemetry...)". Any telemetry reverses that, so it ships
**only** if a concrete optimization question needs real usage data.

**Baseline requirement for every option below:** opt-in only, off by default,
behind a settings toggle. No silent collection, ever.

## Hard constraints (carry into any choice)

- **No `tauri-plugin-shell`** (D6). Telemetry sends over HTTP only — Rust
  `reqwest` or WebView `fetch`. It must never need a subprocess.
- **CSP is `null` today** in [tauri.conf.json](../../crates/agentic-hub/tauri.conf.json).
  Open CSP makes any external script "work", but that's a smell. Whatever endpoint
  we pick should be pinned via a `connect-src` allowlist (and `script-src` if a
  third-party tag is loaded), not left to null CSP.
- **WebView origin is not a real domain.** WKWebView serves from
  `tauri://localhost` / `http://tauri.localhost`. First-party cookies, referrer,
  and page-URL semantics that web analytics assume are unreliable here.
- **Secrets in a distributed binary aren't secret.** Any embedded write key or
  `api_secret` is extractable from the shipped app.

## Comparison axes

For each option: desktop/Tauri fit · key exposure · cookie/origin behavior in the
WebView · CSP impact · consent ergonomics · where it sends from (Rust vs frontend).

### GA4 web tag — `gtag.js` / Google Tag Manager

- **Tauri fit:** poor. Built for real https pages.
- **Cookie/origin:** unreliable. `_ga` first-party cookies, `SameSite`/`Secure`,
  and referrer all misbehave from `tauri://localhost`; client identity and
  sessions drift.
- **CSP impact:** requires loading remote script — needs `script-src` +
  `connect-src` + `img-src` for Google domains, or staying on null CSP.
- **Key exposure:** measurement ID is public anyway.
- **Sends from:** frontend only.
- **Verdict:** not recommended. Web-only assumptions don't hold in a WebView.

### GA4 Measurement Protocol (MP)

- **Tauri fit:** workable — the supported pattern for apps. POST events to
  `https://www.google-analytics.com/mp/collect?measurement_id=…&api_secret=…`.
- **Cookie/origin:** N/A. You self-generate a persisted `client_id` (a UUID),
  no cookies, no origin tricks.
- **CSP impact:** just `connect-src https://www.google-analytics.com`.
- **Key exposure:** `api_secret` ships in the binary and is extractable — anyone
  can forge events into your property. Accept as a known risk.
- **Reporting fidelity:** reduced vs the web tag (some attribution/engagement
  reports stay sparse with MP-only data).
- **Sends from:** Rust `reqwest` preferred (matches "core owns side effects",
  keeps the toggle authoritative) or frontend `fetch`.
- **Verdict:** viable if GA is a hard requirement, but GA gives little a
  desktop-native tool wants.

### Aptabase — Tauri-native

- **Tauri fit:** best. Purpose-built for desktop (Tauri/Electron), official Tauri
  plugin/SDK, anonymous-by-design (no PII, no cross-app identity).
- **Cookie/origin:** N/A — event API, not a web tag.
- **CSP impact:** one `connect-src` to the Aptabase endpoint (cloud or self-host).
- **Key exposure:** app key ships in the binary (same as MP); self-hosting bounds
  the blast radius.
- **Consent ergonomics:** trivial to gate behind the opt-in toggle; nothing fires
  until enabled.
- **Sends from:** frontend SDK, or Rust.
- **Verdict:** recommended primary if we decide to instrument at all.

### PostHog / Plausible / Umami

- **PostHog:** richest (funnels, feature flags); heavier, more identity surface to
  keep anonymous — overkill for a personal local tool.
- **Plausible / Umami:** privacy-first, lightweight, self-hostable; both are
  web-page-oriented, so prefer their event API over a page tag in the WebView.
- **Verdict:** reasonable alternatives; none beats Aptabase's desktop fit, but
  self-hosting (Umami/Plausible) is attractive if data residency matters.

## Decision (2026-06-15)

**Adopted: Aptabase, opt-in, off by default** (decision D15). The desktop shell
tracks `app_started` / `app_exited` from Rust via `tauri-plugin-aptabase`, plus
at most one `daily_active` ping per UTC day when the user actually engages with
the app (window focus, palette summon, dock reopen). Each installation carries a
stable anonymous `clientId` so unique active clients can be counted in Aptabase.
Gated on `Settings.telemetry.enabled`. The WebView never calls out, so there is
no JS binding and no ACL surface. The recommendation below stands as the
rationale.

## Recommendation

1. **Don't instrument until there's a concrete question.** Honor D-open-3. "Nice
   to have analytics" is not a reason to break the no-network posture.
2. **If/when we do:** **Aptabase** (or self-hosted Umami) over GA. GA's value is
   web-funnel analytics this app doesn't have; its desktop story is the awkward MP
   path with an extractable secret.
3. **Whatever we pick:** opt-in toggle off by default, pinned `connect-src`,
   anonymous client id, document exactly which events are sent.

## Open sub-questions

- Which 3-5 metrics would actually change a product decision? (Without these,
  the answer to D-open-3 stays "no".)
- macOS notarization / any store policy disclosure obligations for network calls.
- Cloud vs self-host for data residency and to limit the embedded-key risk.

## Next step

Done: **D-open-3** is resolved by **D15** in [decisions.md](decisions.md), and
the opt-in toggle + Rust-side tracking shipped. Future work, if any, is
expanding the event catalog beyond start/exit — a separate decision.
