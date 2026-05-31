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
add a `RELEASE.md` section, then tag `v<new-version>`.

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
