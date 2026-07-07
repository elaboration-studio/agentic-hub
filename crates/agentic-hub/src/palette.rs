//! Command-palette panel + global-shortcut wiring.
//!
//! The palette is a borderless, transparent, floating panel (Alfred-style)
//! summoned by a user-configurable global accelerator. On macOS it is an
//! `NSPanel` (non-activating, `FullScreenAuxiliary` + `CanJoinAllSpaces`) so it
//! overlays other apps' full-screen Spaces — a plain `NSWindow` cannot, see
//! <https://github.com/tauri-apps/tauri/issues/11488>. On other platforms it is
//! an always-on-top window. All logic here is framework wiring; no domain logic
//! lives in the shell.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Label of the floating command-palette window/panel.
pub const PALETTE_LABEL: &str = "palette";

const PALETTE_WIDTH: f64 = 680.0;
const PALETTE_HEIGHT: f64 = 460.0;

/// Build the hidden palette window. Idempotent: returns the existing window if
/// it was already created. Platform-specific floating behavior is applied by
/// [`setup_palette`].
fn build_palette_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, PALETTE_LABEL, WebviewUrl::App("index.html".into()))
        .title("Command Palette")
        .inner_size(PALETTE_WIDTH, PALETTE_HEIGHT)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .center()
        .build()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{build_palette_window, AppHandle, Manager, PALETTE_LABEL};
    use tauri_nspanel::{
        tauri_panel, CollectionBehavior, ManagerExt, PanelLevel, StyleMask, WebviewWindowExt,
    };

    // A non-activating floating panel: it can become key (so the search field
    // accepts input) without activating the app or switching Spaces.
    tauri_panel! {
        panel!(PalettePanel {
            config: {
                can_become_key_window: true,
                is_floating_panel: true
            }
        })
    }

    /// Create the palette window (if needed) and convert it to a non-activating
    /// floating panel that can overlay full-screen Spaces. Must run on the main
    /// thread (objc window ops); the Tauri `setup` hook satisfies that.
    pub fn setup_palette(app: &AppHandle) -> tauri::Result<()> {
        if app.get_webview_panel(PALETTE_LABEL).is_ok() {
            return Ok(());
        }
        let window = build_palette_window(app)?;
        if let Ok(panel) = window.to_panel::<PalettePanel>() {
            panel.set_level(PanelLevel::Floating.value());
            // Non-activating style mask: summoning never pulls the app's Space
            // forward, so the panel floats over another app's full-screen window.
            panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
            // Collection behavior: join every Space (incl. full-screen Spaces).
            panel.set_collection_behavior(
                CollectionBehavior::new()
                    .full_screen_auxiliary()
                    .can_join_all_spaces()
                    .into(),
            );
            // Changing the style mask above resets the panel to opaque with the
            // default window background, undoing the builder's `transparent(true)`.
            // Re-apply clear background + non-opaque (and drop the native window
            // shadow) so only the rounded CSS card renders — otherwise the native
            // dark background and shadow bleed through the card's corners/edges.
            panel.set_transparent(true);
            panel.set_has_shadow(false);
        }
        Ok(())
    }

    /// Run a closure on the macOS main thread. objc window/panel operations must
    /// not run on the Tokio command threads; this dispatches them safely from any
    /// caller (the global-shortcut and menu handlers are already main-thread).
    fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || f(&handle));
    }

    /// Toggle the palette: hide if visible, otherwise re-center and show as key.
    pub fn toggle_palette(app: &AppHandle) {
        on_main(app, |app| {
            let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) else {
                return;
            };
            if panel.is_visible() {
                panel.hide();
            } else {
                if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
                    let _ = win.center();
                }
                panel.show_and_make_key();
                crate::telemetry::record_client_engagement(app);
            }
        });
    }

    /// Hide the palette panel if it exists (used on blur / dismiss / nav).
    pub fn hide_palette(app: &AppHandle) {
        on_main(app, |app| {
            if let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) {
                panel.hide();
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{build_palette_window, AppHandle, Manager, PALETTE_LABEL};

    /// Create the palette window as an always-on-top, all-workspaces window. On
    /// non-macOS platforms this is the best available floating behavior.
    pub fn setup_palette(app: &AppHandle) -> tauri::Result<()> {
        let win = build_palette_window(app)?;
        let _ = win.set_always_on_top(true);
        let _ = win.set_visible_on_all_workspaces(true);
        Ok(())
    }

    /// Toggle the palette: hide if visible, otherwise re-center, show, and focus.
    pub fn toggle_palette(app: &AppHandle) {
        if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
            if win.is_visible().unwrap_or(false) {
                let _ = win.hide();
            } else {
                let _ = win.center();
                let _ = win.show();
                let _ = win.set_focus();
                crate::telemetry::record_client_engagement(app);
            }
        }
    }

    /// Hide the palette window if it exists (used on blur / dismiss / nav).
    pub fn hide_palette(app: &AppHandle) {
        if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
            let _ = win.hide();
        }
    }
}

pub use imp::{hide_palette, setup_palette, toggle_palette};

/// Re-register the global summon accelerator, replacing any prior registration.
/// Returns a human-readable error when the accelerator string cannot be parsed.
pub fn register_palette_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|_| format!("Invalid shortcut: {accelerator}"))?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(shortcut).map_err(|e| e.to_string())
}
