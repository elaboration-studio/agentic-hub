//! Command-palette panel + global-shortcut wiring.
//!
//! The palette is a borderless, transparent, floating panel (Alfred-style)
//! summoned by a user-configurable global accelerator. On macOS it is an
//! `NSPanel` (non-activating, `FullScreenAuxiliary` + `CanJoinAllSpaces`) so it
//! overlays other apps' full-screen Spaces — a plain `NSWindow` cannot, see
//! <https://github.com/tauri-apps/tauri/issues/11488>. On other platforms it is
//! an always-on-top window. All logic here is framework wiring; no domain logic
//! lives in the shell.

use std::collections::HashSet;
use std::sync::Mutex;

use agentic_core::settings::{
    validate_palette_shortcuts, PaletteLaunchMode, PaletteShortcutValidationError, Settings,
};
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

    /// Show and focus the palette even when it is already visible.
    pub fn show_palette(app: &AppHandle) {
        on_main(app, |app| {
            let Ok(panel) = app.get_webview_panel(PALETTE_LABEL) else {
                return;
            };
            if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
                let _ = win.center();
            }
            panel.show_and_make_key();
            crate::telemetry::record_client_engagement(app);
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

    /// Show and focus the palette even when it is already visible.
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

/// Parsed shortcut-to-launch-mode mapping used by both registration and the
/// global plugin handler. Parsing before settings persistence keeps shortcut
/// strings away from execution sinks and catches alias-equivalent duplicates.
#[derive(Debug, Clone)]
pub struct PaletteShortcutSet {
    entries: Vec<(PaletteLaunchMode, Shortcut)>,
}

impl PaletteShortcutSet {
    pub fn from_settings(settings: &Settings) -> Result<Self, String> {
        validate_palette_shortcuts(settings).map_err(|error| match error {
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

/// OS registration boundary, injectable so rollback is covered without
/// touching the real global shortcut manager in unit tests.
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

/// Replace all shortcuts as one recoverable operation. A failed new
/// registration removes the partial set and restores the prior complete set.
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

/// Persist only after OS registration succeeds. If persistence fails, restore
/// the prior OS set so disk and live shortcuts remain aligned.
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

/// Registered mapping plus the next one-shot palette destination.
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

/// Register the complete set and update dispatch state only after success.
pub fn register_palette_shortcuts(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let state = app.state::<PaletteShortcutState>();
    let previous = state.registered()?;
    let next = PaletteShortcutSet::from_settings(settings)?;
    let mut registrar = AppShortcutRegistrar { app };
    replace_registered_shortcuts(&mut registrar, &previous, &next)?;
    state.set_registered(next)
}

/// Commit shortcuts and their persisted settings as one transaction.
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
