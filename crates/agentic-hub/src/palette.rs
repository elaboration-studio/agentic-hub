//! Command-palette window + global-shortcut wiring. The palette is a borderless,
//! transparent, always-on-top floating window (Alfred-style) summoned by a
//! user-configurable global accelerator. All logic here is Tauri framework
//! wiring; no domain logic lives in the shell.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Label of the floating command-palette window.
pub const PALETTE_LABEL: &str = "palette";

const PALETTE_WIDTH: f64 = 680.0;
const PALETTE_HEIGHT: f64 = 460.0;

/// Create the hidden palette window at launch so it is warm on first summon.
/// Idempotent: returns the existing window if it was already created.
pub fn create_palette_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, PALETTE_LABEL, WebviewUrl::App("index.html".into()))
        .title("Command Palette")
        .inner_size(PALETTE_WIDTH, PALETTE_HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .center()
        .build()
}

/// Bring the palette to the foreground: center, show, focus.
fn show_palette(win: &WebviewWindow) {
    let _ = win.center();
    let _ = win.show();
    let _ = win.set_focus();
}

/// Toggle the palette: hide if visible, otherwise create-if-needed and show.
pub fn toggle_palette(app: &AppHandle) {
    match app.get_webview_window(PALETTE_LABEL) {
        Some(win) => {
            if win.is_visible().unwrap_or(false) {
                let _ = win.hide();
            } else {
                show_palette(&win);
            }
        }
        None => {
            if let Ok(win) = create_palette_window(app) {
                show_palette(&win);
            }
        }
    }
}

/// Hide the palette window if it exists (used on blur / dismiss).
pub fn hide_palette(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
        let _ = win.hide();
    }
}

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
