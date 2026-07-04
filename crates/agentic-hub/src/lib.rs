//! Tauri 2.x desktop shell for Agentic Hub.
//!
//! The WebView is untrusted: this crate only marshals typed payloads to the
//! pure `agentic-core` engine and maps domain errors into [`error::IpcError`].
//! Every command registered here must also appear in `capabilities/default.json`.

mod commands;
mod error;
mod install_window;
mod main_window;
mod menu;
mod palette;
mod paste;
mod telemetry;
mod watcher;

use agentic_core::settings::{default_palette_shortcut, Settings};
use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_aptabase::EventTracker;
use tauri_plugin_global_shortcut::ShortcutState;
use telemetry::TelemetryState;
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
    // The Aptabase plugin starts its flush loop with a bare `tokio::spawn`
    // during plugin setup, which panics without an active Tokio runtime. Tauri
    // does not enter one on the main thread, so we own a runtime here and keep
    // its context entered for the whole app lifetime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("error while building the Tokio runtime");
    let _runtime_guard = runtime.enter();

    with_macos_panel(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Self-update: the updater checks the R2 feed and swaps the bundle; the
        // process plugin relaunches into the new version once installed. The
        // check is driven from the frontend (launch / reopen / weekly).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Telemetry (on by default, user can disable in Config). The plugin is
        // always registered (it sends nothing until `track_event` is called);
        // every call is gated on `TelemetryState`, so nothing leaves the machine
        // while disabled.
        .plugin(tauri_plugin_aptabase::Builder::new(telemetry::APTABASE_KEY).build())
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
        .manage(TelemetryState::default())
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
            let mut settings = Settings::load().unwrap_or_default();
            // 0.8.1 one-time migration: force the watcher on (flipping configs
            // that had paused it) and persist the marker so future user pauses
            // stick. Best-effort save — a failed write only defers the migration.
            if settings.migrate_force_watcher_on() {
                let _ = settings.save();
            }
            // One-time migration: rewrite a stale Codex agents path of
            // `~/.agents/agents` (pre-0.5.0 default, shared with OpenStandard) to
            // the self-contained `~/.codex/agents` so Codex subagent projection
            // lands where Codex actually reads it. Best-effort save.
            if settings.migrate_codex_agents_path() {
                let _ = settings.save();
            }
            // One-time migration: rewrite a stale Antigravity skills path of
            // `~/.gemini/skills` (pre-0.10.1 default) to `~/.gemini/config/skills`
            // where Antigravity actually reads skills. Best-effort save.
            if settings.migrate_antigravity_skills_path() {
                let _ = settings.save();
            }
            if settings.migrate_kiro_rules_agents_md() {
                let _ = settings.save();
            }
            // Start the source watcher on launch when enabled in settings.
            if settings.watcher_enabled {
                app.state::<WatcherState>().start(app.handle().clone());
            }
            // Seed the live telemetry consent flag, then record app start if the
            // user has opted in. No-op (and no network) when disabled.
            let telemetry_on = settings.telemetry.enabled;
            app.state::<TelemetryState>().set_enabled(telemetry_on);
            if telemetry_on {
                let _ = app.track_event("app_started", None);
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
            main_window::restore_main_window(app.handle(), &settings);
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
                if window.label() == main_window::MAIN_LABEL {
                    main_window::persist_main_window(window.app_handle());
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
            commands::cmd_scan_installed_tools,
            commands::cmd_inspect,
            commands::cmd_scaffold_demo,
            commands::cmd_open_path,
            commands::cmd_reveal_path,
            commands::cmd_read_capability_body,
            commands::cmd_paste_to_frontmost,
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
            commands::cmd_suite_apply_preview,
            commands::cmd_set_base_suite,
            commands::cmd_suite_ownership,
            commands::cmd_pick_workspace_dir,
            commands::cmd_list_workspace_targets,
            commands::cmd_remove_workspace_target,
            commands::cmd_set_active_workspace_target,
            commands::cmd_scan_workspace,
            commands::cmd_list_tool_catalog,
            commands::cmd_check_tool,
            commands::cmd_skill_cli_check,
            commands::cmd_search_skills,
            commands::cmd_list_skill_favorites,
            commands::cmd_add_skill_favorite,
            commands::cmd_remove_skill_favorite,
            install_window::cmd_open_install_window,
            install_window::cmd_open_update_window,
            install_window::cmd_take_install_context,
            install_window::cmd_install_skill_stream,
            install_window::cmd_update_skill_stream,
            install_window::cmd_cancel_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Agentic Hub Tauri application")
        // Re-show the hidden window when the user clicks the Dock icon (macOS
        // `applicationShouldHandleReopen`), so a background app is never stranded.
        .run(|app, event| match event {
            RunEvent::Reopen { .. } => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                // Nudge the frontend to run a throttled update scan on re-open.
                let _ = app.emit("app-reopened", ());
            }
            RunEvent::Exit => {
                main_window::persist_main_window(app);
                if app.state::<TelemetryState>().enabled() {
                    let _ = app.track_event("app_exited", None);
                    app.flush_events_blocking();
                }
            }
            _ => {}
        });
}
