//! App-wide native appearance synchronization.
//!
//! The persisted preference lives in `agentic-core::settings`; this module is
//! the only place that translates it into Tauri window and WebView behavior.

use agentic_core::settings::ColorScheme;
use tauri::window::Color;
use tauri::{AppHandle, Manager, Theme, WebviewWindow, Window};

const LIGHT_BACKGROUND: Color = Color(255, 255, 255, 255);
const DARK_BACKGROUND: Color = Color(14, 15, 19, 255);

/// Apply a persisted preference to native chrome and every opaque WebView.
pub fn apply(app: &AppHandle, color_scheme: ColorScheme) {
    app.set_theme(native_theme(color_scheme));
    for window in app.webview_windows().values() {
        let resolved = match color_scheme {
            ColorScheme::System => window.theme().unwrap_or(Theme::Light),
            ColorScheme::Light => Theme::Light,
            ColorScheme::Dark => Theme::Dark,
        };
        sync_webview_background(window, resolved);
    }
}

/// Match the opaque native/WebView background to the resolved theme.
///
/// The command palette deliberately remains transparent so only its floating
/// CSS panel is visible over the active application.
pub fn sync_window_background(window: &Window, theme: Theme) {
    if window.label() == crate::palette::PALETTE_LABEL {
        return;
    }
    let color = background_color(theme);
    let _ = window.set_background_color(Some(color));
}

/// Synchronize both the native window and the WebView surface at startup.
pub fn sync_webview_background(window: &WebviewWindow, theme: Theme) {
    if window.label() == crate::palette::PALETTE_LABEL {
        return;
    }
    let _ = window.set_background_color(Some(background_color(theme)));
}

fn background_color(theme: Theme) -> Color {
    match theme {
        Theme::Dark => DARK_BACKGROUND,
        Theme::Light => LIGHT_BACKGROUND,
        _ => LIGHT_BACKGROUND,
    }
}

fn native_theme(color_scheme: ColorScheme) -> Option<Theme> {
    match color_scheme {
        ColorScheme::System => None,
        ColorScheme::Light => Some(Theme::Light),
        ColorScheme::Dark => Some(Theme::Dark),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_theme_preserves_system_and_explicit_preferences() {
        assert_eq!(native_theme(ColorScheme::System), None);
        assert_eq!(native_theme(ColorScheme::Light), Some(Theme::Light));
        assert_eq!(native_theme(ColorScheme::Dark), Some(Theme::Dark));
    }
}
