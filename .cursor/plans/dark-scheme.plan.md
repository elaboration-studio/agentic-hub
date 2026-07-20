# App-Wide Light, Dark, and System Color Schemes

## Summary

Add an app-wide Appearance setting with **Follow system**, **Light**, and **Dark** options. Changes apply immediately to native Tauri chrome and every WebView: main window, command palette, install window, overlays, dialogs, and toasts.

Existing installations remain Dark when upgrading; fresh installations default to Follow system.

## Interfaces and Implementation

- Add a Rust `ColorScheme` enum with serialized values `"system" | "light" | "dark"` and export it through `ts-rs`.
- Extend `Settings` with `colorScheme: ColorScheme`.
  - `Settings::default()` uses System.
  - Deserializing a legacy config without the field uses Dark.
- Add `cmd_set_color_scheme(colorScheme)` and the broadcast event `color-scheme-changed`, both carrying the generated `ColorScheme` type.
- Centralize native appearance handling in the Tauri shell:
  - Map System to `app.set_theme(None)` and explicit choices to Tauri Light/Dark.
  - Synchronize opaque window backgrounds on startup, preference changes, system-theme events, and before showing the install window.
  - Preserve command-palette transparency.
- Add a tested Zustand appearance store that:
  - Loads the persisted preference before React mounts.
  - Resolves System through `prefers-color-scheme`.
  - Owns the root `.dark` class and CSS `color-scheme`.
  - Reacts to OS changes and `color-scheme-changed` events.
  - Falls back to System if startup settings cannot be read.
- Add an Appearance card at the top of Config using the existing Select primitive. Saving invokes only the dedicated color-scheme command, avoiding unrelated settings side effects.
- Remove forced-dark boot styling and make Sonner consume the resolved theme. Retain the existing canonical light and dark token palettes and audit shadcn dark variants in both modes.

## Delivery

1. Create `codex/app-color-scheme-2026-07-16`.
2. Write Essential specs at `docs/features/appearance-themes.md` and `docs/tech/modules/appearance-theme.md`, update the docs index, commit them, and retain the written-spec review gate before code.
3. Implement test-first in reviewed slices:
   - Settings contract, compatibility defaults, and generated TypeScript types.
   - Native Tauri theme/background synchronization and cross-window event.
   - Zustand controller, Config UI, CSS bootstrap, and resolved-theme toaster.
4. Update `DESIGN.md`, `ARCHITECTURE.md`, the feature registry, and `CHANGELOG.md`. No artificial backlog or roadmap movement is needed because this feature is not currently listed there.
5. Run the approved verification suite, perform task-level and whole-branch reviews, and open a PR to `main`.
6. Treat this user-facing addition as `0.12.0`: replace `RELEASE.md` with the new release notes and bump version files during release preparation; tag and push `v0.12.0` only after PR approval and merge.

## Test Plan

- Rust TDD:
  - Fresh settings default to System.
  - Legacy settings without `colorScheme` load as Dark.
  - JSON values are exactly `system`, `light`, and `dark`.
- Vitest:
  - Explicit Light/Dark applies the correct root class.
  - System follows media-query changes live.
  - Cross-window events update preference and resolved theme.
  - Failed persistence leaves the previous selection active.
  - Initialization is idempotent and fallback behavior is System.
- Verification:
  - Targeted Rust and Vitest tests during implementation.
  - `cargo test --workspace`
  - `pnpm test`
  - `pnpm build`
  - `cargo clippy --all-targets --all-features --locked -- -D warnings`
- Manual Tauri QA:
  - Main Config/Manager/Suites/Resources screens in both themes.
  - Palette, install window, overlays, dialogs, and toasts.
  - Live OS appearance changes in System mode.
  - Persistence across relaunch.
  - Native titlebar/background consistency and absence of paint flashes.

## Assumptions and Review Card

- “White scheme” means the existing canonical light palette with a white canvas and layered light surfaces, not pure white for every element.
- Appearance is app-wide; per-window themes are out of scope.
- No new dependency, shell access, secret, network call, or capability permission is introduced.
- The untrusted WebView can submit only the typed enum; Rust rejects unknown values.
- Appearance data is local, non-sensitive user intent stored in the existing config file; logging, authentication, authorization, crypto, retention, and filesystem projection behavior remain unchanged.
