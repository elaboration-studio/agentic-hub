//! Main window geometry: restore on launch, persist when the user hides it.

use agentic_core::settings::{MainWindowState, Settings};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Position, Size, WebviewWindow};

pub const MAIN_LABEL: &str = "main";

const MIN_WIDTH: u32 = 880;
const MIN_HEIGHT: u32 = 560;

fn clamp_geometry(geom: MainWindowState) -> MainWindowState {
    MainWindowState {
        width: geom.width.max(MIN_WIDTH),
        height: geom.height.max(MIN_HEIGHT),
        x: geom.x,
        y: geom.y,
    }
}

/// Apply saved geometry to the main window, if any.
pub fn restore_main_window(app: &AppHandle, settings: &Settings) {
    let Some(geom) = settings.main_window.clone() else {
        return;
    };
    let Some(win) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    apply_geometry(&win, clamp_geometry(geom));
}

fn apply_geometry(win: &WebviewWindow, geom: MainWindowState) {
    let _ = win.set_size(Size::Logical(LogicalSize {
        width: geom.width as f64,
        height: geom.height as f64,
    }));
    if let (Some(x), Some(y)) = (geom.x, geom.y) {
        let _ = win.set_position(Position::Logical(LogicalPosition {
            x: x as f64,
            y: y as f64,
        }));
    }
}

/// Snapshot the main window's logical geometry into settings (best-effort).
pub fn persist_main_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let Ok(size) = win.inner_size() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0);
    let width = (size.width as f64 / scale).round().max(MIN_WIDTH as f64) as u32;
    let height = (size.height as f64 / scale).round().max(MIN_HEIGHT as f64) as u32;
    let (x, y) = win
        .outer_position()
        .ok()
        .map(|p| {
            (
                Some((p.x as f64 / scale).round() as i32),
                Some((p.y as f64 / scale).round() as i32),
            )
        })
        .unwrap_or((None, None));

    let Ok(mut settings) = Settings::load() else {
        return;
    };
    settings.main_window = Some(MainWindowState {
        width,
        height,
        x,
        y,
    });
    let _ = settings.save();
}
