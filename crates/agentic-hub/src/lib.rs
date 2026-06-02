//! Tauri 2.x desktop shell for Agentic Hub.
//!
//! The WebView is untrusted: this crate only marshals typed payloads to the
//! pure `agentic-core` engine and maps domain errors into [`error::IpcError`].
//! Every command registered here must also appear in `capabilities/default.json`.

mod commands;
mod error;
mod watcher;

use agentic_core::settings::Settings;
use tauri::{Manager, RunEvent, WindowEvent};
use watcher::WatcherState;

/// Build and run the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(WatcherState::default())
        .setup(|app| {
            // Start the source watcher on launch when enabled in settings.
            let enabled = Settings::load().map(|s| s.watcher_enabled).unwrap_or(true);
            if enabled {
                app.state::<WatcherState>().start(app.handle().clone());
            }
            Ok(())
        })
        // Closing the window hides it instead of quitting: the app keeps running
        // in the background (watcher stays live, state is preserved). On macOS
        // Cmd+Q goes through the default Quit menu item, which bypasses this
        // handler and terminates the process — the only intended hard exit.
        .on_window_event(|window, event| {
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
            commands::cmd_rescan_resync,
            commands::cmd_scan,
            commands::cmd_inspect,
            commands::cmd_scaffold_demo,
            commands::cmd_open_path,
            commands::cmd_reveal_path,
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
            commands::cmd_pick_workspace_dir,
            commands::cmd_list_workspace_targets,
            commands::cmd_remove_workspace_target,
            commands::cmd_set_active_workspace_target,
            commands::cmd_apply_workspace_patch,
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
