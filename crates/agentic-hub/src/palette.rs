//! Command-palette lifecycle, active-display placement, and shortcut wiring.
//! macOS uses a non-activating `NSPanel` that can join full-screen Spaces;
//! other platforms use an always-on-top window.

use std::collections::HashSet;
use std::sync::Mutex;

use agentic_core::settings::{
    validate_palette_shortcuts, PaletteLaunchMode, PaletteShortcutValidationError, Settings,
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[cfg(test)]
use crate::palette_presentation::dismissal_after_palette;
pub(crate) use crate::palette_presentation::PalettePresentationState;
use crate::palette_presentation::{
    main_origin_before_palette, PaletteDismissal, PaletteMainOrigin,
};

pub fn should_surface_main_on_reopen(palette_blocks_main: bool) -> bool {
    !palette_blocks_main
}

pub fn palette_blocks_main(app: &AppHandle) -> bool {
    app.state::<PalettePresentationState>().active()
}

pub fn allow_main_window(app: &AppHandle) {
    app.state::<PalettePresentationState>().cancel();
}

fn position_on_active_screen<T>(
    active_screen_origin: Option<T>,
    mut move_to_screen: impl FnMut(T),
    mut native_center: impl FnMut(),
) {
    if let Some(origin) = active_screen_origin {
        move_to_screen(origin);
    }
    native_center();
}

pub const PALETTE_LABEL: &str = "palette";

const PALETTE_WIDTH: f64 = 680.0;
const PALETTE_HEIGHT: f64 = 460.0;

/// Build the hidden palette window, or return the existing one.
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
    use super::{
        build_palette_window, main_origin_before_palette, position_on_active_screen, AppHandle,
        Manager, PaletteDismissal, PaletteMainOrigin, PalettePresentationState, PALETTE_LABEL,
    };
    use tauri_nspanel::{
        objc2_app_kit::{NSApplication, NSScreen},
        objc2_foundation::MainThreadMarker as ObjcMainThreadMarker,
        tauri_panel, CollectionBehavior, ManagerExt, Panel, PanelLevel, StyleMask,
        WebviewWindowExt,
    };

    // A non-activating floating panel: it can become key (so the search field
    // accepts input) without activating the app or switching Spaces.
    tauri_panel! {
        panel!(PalettePanel {
            config: {
                can_become_key_window: true,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }

    /// Create the palette and convert it to a full-screen-capable floating panel.
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

    /// Dispatch objc window operations from any caller to the macOS main thread.
    fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || f(&handle));
    }

    fn center_on_active_screen(panel: &dyn Panel) {
        let origin = ObjcMainThreadMarker::new()
            .and_then(NSScreen::mainScreen)
            .map(|screen| screen.visibleFrame().origin);
        position_on_active_screen(
            origin,
            |screen_origin| panel.as_panel().setFrameOrigin(screen_origin),
            || panel.as_panel().center(),
        );
    }

    fn app_is_hidden() -> bool {
        ObjcMainThreadMarker::new()
            .map(|marker| NSApplication::sharedApplication(marker).isHidden())
            .unwrap_or(false)
    }

    fn main_is_focused(app: &AppHandle) -> bool {
        app.get_webview_window(crate::main_window::MAIN_LABEL)
            .and_then(|main| main.is_focused().ok())
            .unwrap_or(false)
    }

    fn show_panel(app: &AppHandle, panel: &dyn Panel, request: u64) {
        let origin = main_origin_before_palette(app_is_hidden(), main_is_focused(app));
        if !app
            .state::<PalettePresentationState>()
            .start(request, origin)
        {
            return;
        }
        center_on_active_screen(panel);
        panel.show_and_make_key();
        if origin == PaletteMainOrigin::HiddenApp {
            if let Some(main) = app.get_webview_window(crate::main_window::MAIN_LABEL) {
                let _ = main.hide();
            }
        }
        crate::telemetry::record_client_engagement(app);
    }

    fn dismiss_panel(app: &AppHandle, panel: &dyn Panel, request: u64) {
        let Some(dismissal) = app.state::<PalettePresentationState>().complete(request) else {
            return;
        };
        panel.hide();
        match dismissal {
            PaletteDismissal::RestoreHiddenApp => {
                // Re-establish app-level hiding so Cmd+Tab can restore the hub.
                // Both operations are synchronous in this main-thread callback,
                // so the main window is never composited between them.
                if let Some(main) = app.get_webview_window(crate::main_window::MAIN_LABEL) {
                    let _ = main.show();
                }
                let _ = app.hide();
            }
            PaletteDismissal::RefocusMain => {
                if let Some(main) = app.get_webview_window(crate::main_window::MAIN_LABEL) {
                    let _ = main.set_focus();
                }
            }
            PaletteDismissal::None => {}
        }
    }

    pub fn toggle_palette(app: &AppHandle) {
        let Some(request) = app.state::<PalettePresentationState>().request_show() else {
            return;
        };
        on_main(app, move |app| {
            let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) else {
                let _ = app.state::<PalettePresentationState>().complete(request);
                return;
            };
            if panel.is_visible() {
                dismiss_panel(app, panel.as_ref(), request);
            } else {
                show_panel(app, panel.as_ref(), request);
            }
        });
    }

    pub fn hide_palette(app: &AppHandle) {
        let Some(request) = app.state::<PalettePresentationState>().request_hide() else {
            return;
        };
        on_main(app, move |app| {
            if let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) {
                dismiss_panel(app, panel.as_ref(), request);
            } else {
                let _ = app.state::<PalettePresentationState>().complete(request);
            }
        });
    }

    pub fn show_palette(app: &AppHandle) {
        let Some(request) = app.state::<PalettePresentationState>().request_show() else {
            return;
        };
        on_main(app, move |app| {
            let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) else {
                let _ = app.state::<PalettePresentationState>().complete(request);
                return;
            };
            if panel.is_visible() {
                let origin = main_origin_before_palette(app_is_hidden(), main_is_focused(app));
                if !app
                    .state::<PalettePresentationState>()
                    .resume(request, origin)
                {
                    return;
                }
                center_on_active_screen(panel.as_ref());
                panel.show_and_make_key();
            } else {
                show_panel(app, panel.as_ref(), request);
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{build_palette_window, AppHandle, Manager, PALETTE_LABEL};

    pub fn setup_palette(app: &AppHandle) -> tauri::Result<()> {
        let win = build_palette_window(app)?;
        let _ = win.set_always_on_top(true);
        let _ = win.set_visible_on_all_workspaces(true);
        Ok(())
    }

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

    pub fn hide_palette(app: &AppHandle) {
        if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
            let _ = win.hide();
        }
    }

    pub fn show_palette(app: &AppHandle) {
        if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
            let _ = win.center();
            let _ = win.show();
            let _ = win.set_focus();
            crate::telemetry::record_client_engagement(app);
        }
    }
}

pub use imp::{hide_palette, setup_palette, show_palette, toggle_palette};

/// Parsed shortcut-to-launch-mode mapping shared by registration and dispatch.
#[derive(Debug, Clone)]
pub struct PaletteShortcutSet {
    entries: Vec<(PaletteLaunchMode, Shortcut)>,
}

impl PaletteShortcutSet {
    pub fn from_settings(settings: &Settings) -> Result<Self, String> {
        validate_palette_shortcuts(
            &settings.palette_shortcut,
            &settings.palette_quick_search_shortcuts,
        )
        .map_err(|error| match error {
            PaletteShortcutValidationError::Invalid(shortcut) => {
                format!("Invalid shortcut: {shortcut}")
            }
            PaletteShortcutValidationError::Duplicate(shortcut) => {
                format!("Duplicate shortcut: {shortcut}")
            }
        })?;
        let configured = [
            (PaletteLaunchMode::Hub, settings.palette_shortcut.as_str()),
            (
                PaletteLaunchMode::AllResources,
                settings
                    .palette_quick_search_shortcuts
                    .all_resources
                    .as_str(),
            ),
            (
                PaletteLaunchMode::Skills,
                settings.palette_quick_search_shortcuts.skills.as_str(),
            ),
            (
                PaletteLaunchMode::Commands,
                settings.palette_quick_search_shortcuts.commands.as_str(),
            ),
        ];
        let mut entries = Vec::with_capacity(configured.len());
        let mut ids = HashSet::with_capacity(configured.len());
        for (mode, accelerator) in configured {
            let shortcut: Shortcut = accelerator
                .parse()
                .map_err(|_| format!("Invalid shortcut: {accelerator}"))?;
            if !ids.insert(shortcut.id()) {
                return Err(format!("Duplicate shortcut: {accelerator}"));
            }
            entries.push((mode, shortcut));
        }
        Ok(Self { entries })
    }

    fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn shortcuts(&self) -> Vec<Shortcut> {
        self.entries.iter().map(|(_, shortcut)| *shortcut).collect()
    }

    #[cfg(test)]
    pub fn shortcut_for(&self, mode: PaletteLaunchMode) -> Option<&Shortcut> {
        self.entries
            .iter()
            .find_map(|(entry_mode, shortcut)| (*entry_mode == mode).then_some(shortcut))
    }

    pub fn mode_for(&self, shortcut: &Shortcut) -> Option<PaletteLaunchMode> {
        self.entries
            .iter()
            .find_map(|(mode, registered)| (registered.id() == shortcut.id()).then_some(*mode))
    }
}

/// Injectable OS registration boundary for rollback tests.
pub trait ShortcutRegistrar {
    fn unregister_all(&mut self) -> Result<(), String>;
    fn register(&mut self, shortcut: Shortcut) -> Result<(), String>;
}

struct AppShortcutRegistrar<'a> {
    app: &'a AppHandle,
}

impl ShortcutRegistrar for AppShortcutRegistrar<'_> {
    fn unregister_all(&mut self) -> Result<(), String> {
        self.app
            .global_shortcut()
            .unregister_all()
            .map_err(|error| error.to_string())
    }

    fn register(&mut self, shortcut: Shortcut) -> Result<(), String> {
        self.app
            .global_shortcut()
            .register(shortcut)
            .map_err(|error| error.to_string())
    }
}

/// Replace all shortcuts, restoring the prior set if registration fails.
pub fn replace_registered_shortcuts(
    registrar: &mut impl ShortcutRegistrar,
    previous: &PaletteShortcutSet,
    next: &PaletteShortcutSet,
) -> Result<(), String> {
    registrar.unregister_all()?;
    for shortcut in next.shortcuts() {
        if let Err(register_error) = registrar.register(shortcut) {
            let rollback = registrar.unregister_all().and_then(|()| {
                for prior in previous.shortcuts() {
                    registrar.register(prior)?;
                }
                Ok(())
            });
            return match rollback {
                Ok(()) => Err(register_error),
                Err(rollback_error) => Err(format!(
                    "{register_error}; failed to restore previous shortcuts: {rollback_error}"
                )),
            };
        }
    }
    Ok(())
}

/// Persist after registration, restoring prior shortcuts if persistence fails.
pub fn replace_registered_shortcuts_and_persist(
    registrar: &mut impl ShortcutRegistrar,
    previous: &PaletteShortcutSet,
    next: &PaletteShortcutSet,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    replace_registered_shortcuts(registrar, previous, next)?;
    if let Err(persist_error) = persist() {
        return match replace_registered_shortcuts(registrar, next, previous) {
            Ok(()) => Err(format!("Settings save failed: {persist_error}")),
            Err(rollback_error) => Err(format!(
                "Settings save failed: {persist_error}; failed to restore previous shortcuts: {rollback_error}"
            )),
        };
    }
    Ok(())
}

#[derive(Debug)]
pub struct PaletteShortcutState {
    registered: Mutex<PaletteShortcutSet>,
    launch_mode: Mutex<PaletteLaunchMode>,
}

impl Default for PaletteShortcutState {
    fn default() -> Self {
        Self {
            registered: Mutex::new(PaletteShortcutSet::empty()),
            launch_mode: Mutex::new(PaletteLaunchMode::Hub),
        }
    }
}

impl PaletteShortcutState {
    fn registered(&self) -> Result<PaletteShortcutSet, String> {
        self.registered
            .lock()
            .map(|set| set.clone())
            .map_err(|_| "palette shortcut state unavailable".to_string())
    }

    fn set_registered(&self, set: PaletteShortcutSet) -> Result<(), String> {
        let mut registered = self
            .registered
            .lock()
            .map_err(|_| "palette shortcut state unavailable".to_string())?;
        *registered = set;
        Ok(())
    }

    pub fn request_launch(&self, mode: PaletteLaunchMode) {
        if let Ok(mut launch_mode) = self.launch_mode.lock() {
            *launch_mode = mode;
        }
    }

    pub fn take_launch_mode(&self) -> PaletteLaunchMode {
        self.launch_mode
            .lock()
            .map(|mut mode| std::mem::take(&mut *mode))
            .unwrap_or_default()
    }

    fn mode_for(&self, shortcut: &Shortcut) -> Option<PaletteLaunchMode> {
        self.registered
            .lock()
            .ok()
            .and_then(|set| set.mode_for(shortcut))
    }
}

pub fn dispatch_palette_shortcut(
    state: &PaletteShortcutState,
    mode: PaletteLaunchMode,
    toggle: impl FnOnce(),
    show: impl FnOnce(),
) {
    if mode == PaletteLaunchMode::Hub {
        toggle();
    } else {
        state.request_launch(mode);
        show();
    }
}

/// Dispatch one registered key press. Unknown shortcuts are ignored.
pub fn handle_shortcut_press(app: &AppHandle, shortcut: &Shortcut) {
    let state = app.state::<PaletteShortcutState>();
    let Some(mode) = state.mode_for(shortcut) else {
        return;
    };
    dispatch_palette_shortcut(&state, mode, || toggle_palette(app), || show_palette(app));
}

pub fn register_palette_shortcuts(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let state = app.state::<PaletteShortcutState>();
    let previous = state.registered()?;
    let next = PaletteShortcutSet::from_settings(settings)?;
    let mut registrar = AppShortcutRegistrar { app };
    replace_registered_shortcuts(&mut registrar, &previous, &next)?;
    state.set_registered(next)
}

pub fn register_palette_shortcuts_and_persist(
    app: &AppHandle,
    settings: &Settings,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let state = app.state::<PaletteShortcutState>();
    let previous = state.registered()?;
    let next = PaletteShortcutSet::from_settings(settings)?;
    let mut registrar = AppShortcutRegistrar { app };
    replace_registered_shortcuts_and_persist(&mut registrar, &previous, &next, persist)?;
    state.set_registered(next)
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
