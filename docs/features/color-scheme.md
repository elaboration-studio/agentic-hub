# Feature: App-wide Color Scheme

Status: Implemented
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-17
Depends On: [DESIGN.md](../../DESIGN.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [Tauri IPC contract](../tech/modules/tauri-ipc-contract.md)

## User story

As a desktop-app user, I can choose Light, Dark, or Follow system in Config so
every Agentic Hub window matches my preferred appearance without restarting the
app.

## Behavior

- **Follow system** is the default for fresh settings and resolves through the
  operating system's current appearance. A live system change updates every UI
  surface and native window background.
- **Light** and **Dark** override the operating system immediately.
- Existing `~/.agentic-hub/config.json` files that predate `colorScheme` retain
  the previous dark appearance. Saving a preference writes the new field using
  the stable wire values `system`, `light`, and `dark`.
- The main window, floating command palette, install window, toasts, and native
  Tauri chrome stay in sync. The transparent palette shell remains transparent;
  its rendered panel receives the selected CSS tokens.

## Architecture

`Settings.colorScheme` is a Rust-owned `ColorScheme` enum exported to TypeScript
through `ts-rs`. `cmd_set_color_scheme` is intentionally separate from the
generic settings form save: it persists the preference, applies native chrome,
and broadcasts `color-scheme-changed` to every WebView. Generic saves preserve
this on-disk value so a stale Config form cannot undo an appearance change.

Each renderer initializes its appearance store before React mounts. The main
window remains hidden until that initialization is complete, avoiding a visible
wrong-theme frame. The store applies the resolved scheme to the document root,
listens for `prefers-color-scheme` changes while following the system, and reacts
to the cross-window Tauri event.

## Verification

- Rust settings tests cover fresh defaults, legacy deserialization, and stable
  serialized values.
- Zustand store tests cover persisted preference, OS changes, cross-window
  synchronization, persistence failures, and settings-load fallback.
- Type-check and build gates verify the typed IPC contract and native shell code.
