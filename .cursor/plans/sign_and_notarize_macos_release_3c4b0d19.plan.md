---
name: Sign and notarize macOS release
overview: Sign the macOS DMG with your Developer ID Application certificate and notarize it via the App Store Connect API key inside the existing release workflow, so the app installs without the Gatekeeper "Privacy & Security" prompt.
todos:
  - id: workflow
    content: "Edit .github/workflows/release.yml: add 'Write App Store Connect API key' step and env block (cert + signing identity + API key vars) on the build step"
    status: completed
  - id: config
    content: Optionally add bundle.macOS.minimumSystemVersion to crates/agentic-hub/tauri.conf.json
    status: completed
isProject: false
---

# Sign and Notarize the macOS Release

The DMG from [.github/workflows/release.yml](.github/workflows/release.yml) is unsigned/un-notarized, so macOS Gatekeeper quarantines it. Fix = sign with a **Developer ID Application** cert + **notarize** with an App Store Connect API key, both wired into CI via env vars. Tauri auto-imports the cert into a temp keychain, so no manual `security` steps are needed.

There are two parts: (1) one-time manual setup you do in your Apple account + GitHub, and (2) the small code changes I make.

## Part 1 - One-time manual setup (you do this)

### 1a. Create the Developer ID Application certificate
- In **Xcode -> Settings -> Accounts**, select your team -> **Manage Certificates -> + -> Developer ID Application** (or create it at [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates/list)).
- It installs into your login Keychain.

### 1b. Export the cert as `.p12`
- **Keychain Access -> My Certificates**, find `Developer ID Application: <Name> (TEAMID)`, expand it (must show the private key), right-click -> **Export** -> `.p12`, set a password (remember it).
- Base64-encode it:

```bash
base64 -i Certificate.p12 | pbcopy   # now in clipboard for the secret
```

- Note the full identity string and your 10-char Team ID:

```bash
security find-identity -v -p codesigning   # copy "Developer ID Application: Name (TEAMID)"
```

### 1c. Create an App Store Connect API key (for notarization)
- [App Store Connect -> Users and Access -> Integrations -> App Store Connect API](https://appstoreconnect.apple.com/access/integrations/api) -> **+**, give it **Developer** access.
- Record the **Issuer ID** (above the table) and the **Key ID** (table row).
- Download the `.p8` private key (one-time download). Base64-encode it:

```bash
base64 -i AuthKey_XXXXXXXX.p8 | pbcopy
```

### 1d. Add GitHub repository secrets
Repo -> **Settings -> Secrets and variables -> Actions -> New repository secret**:

- `APPLE_CERTIFICATE` - base64 of the `.p12` (from 1b)
- `APPLE_CERTIFICATE_PASSWORD` - the `.p12` export password
- `APPLE_SIGNING_IDENTITY` - e.g. `Developer ID Application: Your Name (TEAMID)`
- `APPLE_API_ISSUER` - Issuer ID (1c)
- `APPLE_API_KEY` - Key ID (1c)
- `APPLE_API_KEY_BASE64` - base64 of the `.p8` (1c)

## Part 2 - Code changes (I do this)

### 2a. Update [.github/workflows/release.yml](.github/workflows/release.yml)
- Add a step (before the build) that decodes `APPLE_API_KEY_BASE64` into a `.p8` file under `$RUNNER_TEMP`.
- Add an `env:` block to the existing "Build macOS universal bundle" step so Tauri signs + notarizes:

```yaml
      - name: Write App Store Connect API key
        env:
          APPLE_API_KEY_BASE64: ${{ secrets.APPLE_API_KEY_BASE64 }}
        run: echo "$APPLE_API_KEY_BASE64" | base64 --decode > "$RUNNER_TEMP/AuthKey.p8"

      - name: Build macOS universal bundle
        env:
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_API_ISSUER: ${{ secrets.APPLE_API_ISSUER }}
          APPLE_API_KEY: ${{ secrets.APPLE_API_KEY }}
          APPLE_API_KEY_PATH: ${{ runner.temp }}/AuthKey.p8
        run: pnpm tauri build --target universal-apple-darwin
```

Everything else in the workflow (publish step, draft release) stays the same.

### 2b. (Optional) Pin minimum macOS version in [crates/agentic-hub/tauri.conf.json](crates/agentic-hub/tauri.conf.json)
Hardened runtime is applied automatically when signing; no entitlements file is needed for this app's current capabilities. Optionally add a `bundle.macOS` block for a clean floor:

```json
  "bundle": {
    "macOS": { "minimumSystemVersion": "11.0" }
  }
```

## Part 3 - Verify
- Tag a release: `git tag v0.1.2 && git push origin v0.1.2`.
- After CI finishes, download the DMG from the draft release on a clean Mac (or run `xattr -dr com.apple.quarantine` is NOT needed anymore).
- Confirm: `spctl -a -vvv -t install /Applications/Agentic\ Hub.app` -> should report `accepted` / `source=Notarized Developer ID`.
- App should open with no "Privacy & Security" detour.

## Notes
- The Apple ID + app-specific-password method is an alternative to the API key, but you chose the API key (no 2FA hassle, recommended).
- If notarization ever fails in CI, the build log prints a submission ID; `xcrun notarytool log <id>` shows the reason (usually a missing hardened-runtime/entitlement issue).