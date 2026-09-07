//! Native application menu. Provides the standard macOS menu surface (App,
//! Edit, View, Window) plus two custom items: "Settings…" (Cmd+,) which routes
//! the main window to Config, and "Command Palette" which toggles the palette.
//! `PredefinedMenuItem::quit` is kept so Cmd+Q remains the hard-exit that
//! bypasses the close-to-hide handler.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

pub const ID_SETTINGS: &str = "settings";
pub const ID_PALETTE: &str = "command-palette";
pub const ID_CHECK_UPDATES: &str = "check-updates";

/// Build the full application menu.
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let app_name = app.package_info().name.clone();

    let settings_item =
        MenuItem::with_id(app, ID_SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?;
    let palette_item = MenuItem::with_id(app, ID_PALETTE, "Command Palette", true, None::<&str>)?;
    let check_updates_item = MenuItem::with_id(
        app,
        ID_CHECK_UPDATES,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;

    let app_menu = Submenu::with_items(
        app,
        app_name,
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &check_updates_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(app, "View", true, &[&palette_item])?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &edit_menu, &view_menu, &window_menu])
}

/// Route menu activations. Unknown ids are ignored.
pub fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        ID_SETTINGS => {
            crate::palette::allow_main_window(app);
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            let _ = app.emit("menu-open-config", ());
        }
        ID_PALETTE => crate::palette::toggle_palette(app),
        ID_CHECK_UPDATES => {
            crate::palette::allow_main_window(app);
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            let _ = app.emit("menu-check-updates", ());
        }
        _ => {}
    }
}
