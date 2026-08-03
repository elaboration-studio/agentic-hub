# Deployment

How Agentic Hub is built and released. This file is the source of truth for
release configuration; the release workflow in
[`.github/workflows/release.yml`](.github/workflows/release.yml) implements it.

## Platform

- **Provider:** GitHub Actions (build) + GitHub Releases (distribution)
- **Project type:** Tauri 2.x desktop app
- **Targets (this version):** macOS only — universal binary (Apple Silicon + Intel)

## Artifacts

- **`Agentic Hub_<version>_universal.dmg`** — the user-facing macOS installer.
  Produced at `target/universal-apple-darwin/release/bundle/dmg/`.
- **`Agentic Hub.app.tar.gz`** + **`.sig`** — the Tauri *updater* payload and its
  minisign signature, produced at `target/universal-apple-darwin/release/bundle/macos/`
  because `bundle.createUpdaterArtifacts` is `true`. The `.dmg` is for first
  install; the `.app.tar.gz` is what the in-app updater downloads.
- **`latest.json`** — the updater feed (assembled in CI), hosted on R2.
- The `.dmg` contains `Agentic Hub.app`. Linux/Windows bundles are out of scope
  for now (the workflow only runs on macOS).

## Release flow

- **Trigger:** push a version tag matching `v*`.
- **Workflow:** `.github/workflows/release.yml` (job `macos`, `runs-on: macos-latest`).
- **Steps:** install pnpm/Node 20/Rust stable (both Apple arches) →
  `pnpm install --frozen-lockfile` → `pnpm tauri build --target universal-apple-darwin`
  → publish a **draft** GitHub Release with the `.dmg` attached.
- **Why build from the repo root:** the Tauri config lives at
  `crates/agentic-hub/tauri.conf.json` (not `src-tauri/`). Running from the root
  lets the Tauri CLI auto-discover it and resolves the root-relative
  `beforeBuildCommand` (`pnpm build` → `dist/`). Do not `cd` into the crate.
- **Release is a draft:** review the generated notes and assets, then publish
  manually from the GitHub Releases page.

## Cutting a release

```bash
# 1. Ensure the version is consistent and main is up to date.
#    package.json, crates/agentic-hub/tauri.conf.json, and both Cargo.toml
#    files must all read the same version (e.g. 0.1.0).

# 2. Tag on main and push the tag (this fires the workflow).
git checkout main && git pull
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# 3. Watch the build, then publish the draft release.
gh run watch
gh release view v0.1.0 --web
```

To bump for the next release, raise the version in all four files in lockstep,
append the new entry to `CHANGELOG.md`, **replace** `RELEASE.md` with the new
version's notes (it is published verbatim as the GitHub Release body, so it must
hold only the current release), then tag `v<new-version>`.

## Auto-update (Tauri updater + Cloudflare R2)

Agentic Hub updates itself with the **built-in Tauri updater** (not Sparkle —
that's for native Swift apps). The app checks an R2-hosted feed, downloads a
minisign-signed bundle, verifies it, and relaunches.

### How it works

```
app (launch / re-open / weekly)
  -> GET https://pub-96a02606d38546a2a0d158c71c45d99b.r2.dev/agentic-hub/latest.json
  -> newer version? download .app.tar.gz -> verify signature -> relaunch
```

- **Trigger cadence (frontend, [src/state/update.ts](src/state/update.ts)):** a
  check runs on launch, on each app re-open (the `RunEvent::Reopen` Dock click
  emits `app-reopened`), and on a weekly interval — all throttled to at most
  once per 7 days via a persisted `lastCheckedAt`. The **App ▸ Check for
  Updates…** menu item forces an immediate, non-silent check.
- **Integrity:** Apple notarization + the Tauri **minisign** signature. HTTPS
  protects transport; the signature protects the payload even if hosting is
  compromised. The public key is embedded in
  [`tauri.conf.json`](crates/agentic-hub/tauri.conf.json) (`plugins.updater.pubkey`).

### R2 layout

Bucket `estudio-ehub`, prefix `agentic-hub/`, served via the bucket's public
`r2.dev` domain:

```
agentic-hub/latest.json                                (feed; Cache-Control max-age=60)
agentic-hub/Agentic-Hub-<version>-universal.app.tar.gz (updater payload)
agentic-hub/Agentic Hub_<version>_universal.dmg        (first-install download)
```

### latest.json schema

Both arch keys point at the single universal tarball:

```json
{
  "version": "0.9.3",
  "notes": "…RELEASE.md…",
  "pub_date": "2026-06-24T12:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "<sig>", "url": "https://pub-96a02606d38546a2a0d158c71c45d99b.r2.dev/agentic-hub/Agentic-Hub-0.9.3-universal.app.tar.gz" },
    "darwin-x86_64":  { "signature": "<sig>", "url": "https://pub-96a02606d38546a2a0d158c71c45d99b.r2.dev/agentic-hub/Agentic-Hub-0.9.3-universal.app.tar.gz" }
  }
}
```

### CI sync

`release.yml` (after the build) generates `latest.json` from the `.sig` +
`RELEASE.md`, then `aws s3 cp`s the tarball, dmg, and feed to R2's
S3-compatible endpoint. No manual upload step — tag and the feed updates.

### One-time setup (cannot be done by the Cloudflare MCP — it manages buckets, not objects)

1. **Updater keypair** — already generated at `~/.tauri/agentic-hub-updater.key`
   (public key embedded in `tauri.conf.json`). To regenerate:
   `pnpm tauri signer generate -w ~/.tauri/agentic-hub-updater.key`.
2. **R2 API token** — in the Cloudflare dashboard, create an R2 token with
   object read+write on `estudio-ehub` (gives an S3 access key id + secret).
3. **Enable the bucket's public `r2.dev` URL** (R2 ▸ bucket ▸ Settings).
4. **Repo secrets / variables** (`gh secret set` / `gh variable set`):
   - secret `TAURI_SIGNING_PRIVATE_KEY` — contents of `~/.tauri/agentic-hub-updater.key`
   - secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the key's password
   - secret `R2_ACCESS_KEY_ID`, secret `R2_SECRET_ACCESS_KEY`
   - variable `R2_BUCKET` = `estudio-ehub`
5. **eHub landing page sync (optional but recommended):** secret
   `EHUB_SYNC_TOKEN` — a GitHub PAT with `contents:write` on
   `SurfaceW/e-studio-hubs`. After each release upload to R2,
   `release.yml` dispatches `agentic-hub-release` so the marketing site commits
   the new download version and Vercel redeploys. Without it, the site still
   picks up the version on its next `prebuild` (fetches R2 `latest.json`).

## Health check

Desktop app — no live URL. "Healthy" means:

- The workflow finishes green and uploads exactly one `.dmg`
  (`fail_on_unmatched_files: true` guards against a missing artifact).
- The `.dmg` mounts and `Agentic Hub.app` launches on a clean macOS machine.

```bash
gh run list --workflow release.yml -L 1   # latest release run status
```

## Credentials & secrets

- **`GITHUB_TOKEN`** — provided automatically by GitHub Actions; the workflow
  needs `contents: write` to create the release (already declared).
- **Updater + R2 (required for release builds):** `TAURI_SIGNING_PRIVATE_KEY`,
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`
  (secrets) and `R2_BUCKET` (variable). See *Auto-update* above. With
  `createUpdaterArtifacts: true`, a missing signing key fails the build.
- **No code-signing today.** Builds are unsigned, so first launch needs
  right-click ▸ Open (or System Settings ▸ Privacy & Security ▸ Open Anyway).
  - *Future:* to sign + notarize, add `APPLE_CERTIFICATE`,
    `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
    `APPLE_PASSWORD`, and `APPLE_TEAM_ID` as repo secrets and wire them into the
    build step's env. Never commit secret values.

## Notes

- `bundle.active` must stay `true` in `tauri.conf.json` — with it `false`,
  `tauri build` produces no `.dmg` and the release step fails on the empty glob.
- Windows/Linux support is a follow-up: add an OS matrix and switch the publish
  step to `tauri-apps/tauri-action` (or per-OS upload) when that lands.
