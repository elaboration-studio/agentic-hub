//! Tauri 2.x desktop shell for Agentic Hub.
//!
//! The WebView is untrusted: this crate only marshals typed payloads to the
//! pure `agentic-core` engine and maps domain errors into [`error::IpcError`].
//! Every command registered here must also appear in `capabilities/default.json`.

mod commands;
mod error;
mod install_window;
mod menu;
mod palette;
mod watcher;

use agentic_core::settings::{default_palette_shortcut, Settings};
use tauri::webview::PageLoadEvent;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use watcher::WatcherState;

/// Register the `tauri-nspanel` plugin on macOS so the palette window can be
/// subclassed to a non-activating `NSPanel` (no-op on other platforms).
fn with_macos_panel(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    #[cfg(target_os = "macos")]
    {
        builder.plugin(tauri_nspanel::init())
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder
    }
}

/// Build and run the Tauri application.
pub fn run() {
    with_macos_panel(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Global summon accelerator for the command palette. The handler fires
        // for any registered shortcut; we only ever register the palette one.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        palette::toggle_palette(app);
                    }
                })
                .build(),
        )
        .manage(WatcherState::default())
        .manage(install_window::InstallContextState::default())
        .manage(install_window::InstallState::default())
        .menu(menu::build_menu)
        .on_menu_event(menu::handle_menu_event)
        // The main window starts hidden (tauri.conf.json `visible: false`) to
        // avoid a white paint flash before the WebView renders. Show it only
        // once the page has finished loading.
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && payload.event() == PageLoadEvent::Finished {
                let _ = webview.window().show();
            }
        })
        .setup(|app| {
            let settings = Settings::load().unwrap_or_default();
            // Start the source watcher on launch when enabled in settings.
            if settings.watcher_enabled {
                app.state::<WatcherState>().start(app.handle().clone());
            }
            // Pre-create the (hidden) palette panel so the first summon is
            // instant, then register the configured global accelerator. A bad
            // saved accelerator falls back to the default so summon never breaks.
            let _ = palette::setup_palette(app.handle());
            if palette::register_palette_shortcut(app.handle(), &settings.palette_shortcut).is_err()
            {
                let _ =
                    palette::register_palette_shortcut(app.handle(), &default_palette_shortcut());
            }
            Ok(())
        })
        // Closing the window hides it instead of quitting: the app keeps running
        // in the background (watcher stays live, state is preserved). On macOS
        // Cmd+Q goes through the default Quit menu item, which bypasses this
        // handler and terminates the process — the only intended hard exit.
        .on_window_event(|window, event| {
            // Alfred-style dismiss: the palette hides as soon as it loses focus.
            if window.label() == palette::PALETTE_LABEL {
                if let WindowEvent::Focused(false) = event {
                    palette::hide_palette(window.app_handle());
                }
                return;
            }
            // The install window closes normally (it is rebuilt per open), but
            // kill any in-flight install so an abandoned window never strands a
            // running `npx`.
            if window.label() == install_window::INSTALL_LABEL {
                if let WindowEvent::CloseRequested { .. } = event {
                    install_window::on_install_window_closed(window.app_handle());
                }
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    // Hide the whole application (NSApp hide:), not just the
                    // window. App-level hide lets Cmd+Tab and the Dock icon
                    // re-activate and restore the window the macOS-native way.
                    // Ordering only the window out leaves the app windowless, and
                    // Cmd+Tab to a windowless app shows nothing.
                    #[cfg(target_os = "macos")]
                    let _ = window.app_handle().hide();
                    #[cfg(not(target_os = "macos"))]
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::cmd_load_settings,
            commands::cmd_save_settings,
            commands::cmd_set_watcher_enabled,
            commands::cmd_toggle_palette,
            commands::cmd_show_main,
            commands::cmd_rescan_resync,
            commands::cmd_scan,
            commands::cmd_inspect,
            commands::cmd_scaffold_demo,
            commands::cmd_open_path,
            commands::cmd_reveal_path,
            commands::cmd_read_capability_body,
            commands::cmd_open_url,
            commands::cmd_add_source,
            commands::cmd_remove_source,
            commands::cmd_plan,
            commands::cmd_apply,
            commands::cmd_sync_rules,
            commands::cmd_sync_hooks,
            commands::cmd_list_suites,
            commands::cmd_get_suite,
            commands::cmd_create_suite,
            commands::cmd_update_suite,
            commands::cmd_delete_suite,
            commands::cmd_apply_suite,
            commands::cmd_set_base_suite,
            commands::cmd_suite_ownership,
            commands::cmd_pick_workspace_dir,
            commands::cmd_list_workspace_targets,
            commands::cmd_remove_workspace_target,
            commands::cmd_set_active_workspace_target,
            commands::cmd_scan_workspace,
            commands::cmd_skill_cli_check,
            commands::cmd_search_skills,
            commands::cmd_list_skill_favorites,
            commands::cmd_add_skill_favorite,
            commands::cmd_remove_skill_favorite,
            install_window::cmd_open_install_window,
            install_window::cmd_take_install_context,
            install_window::cmd_install_skill_stream,
            install_window::cmd_cancel_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Agentic Hub Tauri application")
        // Re-show the hidden window when the user clicks the Dock icon (macOS
        // `applicationShouldHandleReopen`), so a background app is never stranded.
        .run(|app, event| {
            if let RunEvent::Reopen { .. } = event {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
}
