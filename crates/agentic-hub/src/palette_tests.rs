use std::cell::RefCell;
use std::collections::HashSet;

use agentic_core::settings::{PaletteLaunchMode, Settings};

use super::{
    dismissal_after_palette, dispatch_palette_shortcut, main_origin_before_palette,
    position_on_active_screen, replace_registered_shortcuts,
    replace_registered_shortcuts_and_persist, should_surface_main_on_reopen, PaletteDismissal,
    PaletteMainOrigin, PalettePresentationState, PaletteShortcutSet, PaletteShortcutState,
    ShortcutRegistrar,
};

#[derive(Default)]
struct FakeRegistrar {
    registered: HashSet<u32>,
    fail_on: Option<u32>,
}

impl ShortcutRegistrar for FakeRegistrar {
    fn unregister_all(&mut self) -> Result<(), String> {
        self.registered.clear();
        Ok(())
    }

    fn register(&mut self, shortcut: tauri_plugin_global_shortcut::Shortcut) -> Result<(), String> {
        if self.fail_on == Some(shortcut.id()) {
            return Err("shortcut unavailable".to_string());
        }
        self.registered.insert(shortcut.id());
        Ok(())
    }
}

fn ids(set: &PaletteShortcutSet) -> HashSet<u32> {
    set.shortcuts()
        .iter()
        .map(|shortcut| shortcut.id())
        .collect()
}

#[test]
fn replacing_shortcuts_rolls_back_the_complete_prior_set_when_registration_fails() {
    let previous = PaletteShortcutSet::from_settings(&Settings::default()).unwrap();
    let mut next_settings = Settings::default();
    next_settings.palette_quick_search_shortcuts.skills = "Cmd+Alt+Ctrl+K".to_string();
    let next = PaletteShortcutSet::from_settings(&next_settings).unwrap();
    let failing_id = next.shortcut_for(PaletteLaunchMode::Skills).unwrap().id();
    let mut registrar = FakeRegistrar {
        registered: ids(&previous),
        fail_on: Some(failing_id),
    };

    let result = replace_registered_shortcuts(&mut registrar, &previous, &next);

    assert!(result.is_err());
    assert_eq!(registrar.registered, ids(&previous));
}

#[test]
fn failed_registration_does_not_persist_new_settings() {
    let previous = PaletteShortcutSet::from_settings(&Settings::default()).unwrap();
    let mut next_settings = Settings::default();
    next_settings.palette_quick_search_shortcuts.skills = "Cmd+Alt+Ctrl+K".to_string();
    let next = PaletteShortcutSet::from_settings(&next_settings).unwrap();
    let failing_id = next.shortcut_for(PaletteLaunchMode::Skills).unwrap().id();
    let mut registrar = FakeRegistrar {
        registered: ids(&previous),
        fail_on: Some(failing_id),
    };
    let mut persisted = false;

    let result = replace_registered_shortcuts_and_persist(&mut registrar, &previous, &next, || {
        persisted = true;
        Ok(())
    });

    assert!(result.is_err());
    assert!(!persisted);
    assert_eq!(registrar.registered, ids(&previous));
}

#[test]
fn failed_settings_persistence_restores_prior_shortcuts() {
    let previous = PaletteShortcutSet::from_settings(&Settings::default()).unwrap();
    let mut next_settings = Settings::default();
    next_settings.palette_quick_search_shortcuts.skills = "Cmd+Alt+Ctrl+K".to_string();
    let next = PaletteShortcutSet::from_settings(&next_settings).unwrap();
    let mut registrar = FakeRegistrar {
        registered: ids(&previous),
        fail_on: None,
    };

    let result = replace_registered_shortcuts_and_persist(&mut registrar, &previous, &next, || {
        Err("disk full".to_string())
    });

    assert!(result.is_err());
    assert_eq!(registrar.registered, ids(&previous));
}

#[test]
fn shortcut_set_rejects_aliases_that_parse_to_the_same_accelerator() {
    let mut settings = Settings::default();
    settings.palette_quick_search_shortcuts.commands = "Command+Option+A".to_string();

    let result = PaletteShortcutSet::from_settings(&settings);

    assert!(result.is_err());
}

#[test]
fn direct_shortcut_dispatches_requested_mode_and_always_shows_palette() {
    let set = PaletteShortcutSet::from_settings(&Settings::default()).unwrap();
    let shortcut = set.shortcut_for(PaletteLaunchMode::Commands).unwrap();
    let state = PaletteShortcutState::default();
    let mut toggled = false;
    let mut shown = 0;

    dispatch_palette_shortcut(
        &state,
        PaletteLaunchMode::Commands,
        || toggled = true,
        || shown += 1,
    );
    dispatch_palette_shortcut(
        &state,
        set.mode_for(shortcut).unwrap(),
        || toggled = true,
        || shown += 1,
    );

    assert!(!toggled);
    assert_eq!(shown, 2);
    assert_eq!(state.take_launch_mode(), PaletteLaunchMode::Commands);
}

#[test]
fn reopen_does_not_surface_main_while_palette_blocks_it() {
    assert!(!should_surface_main_on_reopen(true));
    assert!(should_surface_main_on_reopen(false));
}

#[test]
fn presentation_blocks_reopen_before_main_thread_captures_origin() {
    let state = PalettePresentationState::default();

    let request = state.request_show().unwrap();

    assert!(!should_surface_main_on_reopen(state.active()));
    assert!(state.start(request, PaletteMainOrigin::HiddenApp));
    assert_eq!(
        state.complete(request),
        Some(PaletteDismissal::RestoreHiddenApp)
    );
}

#[test]
fn cancelled_pending_presentation_rejects_a_queued_show() {
    let state = PalettePresentationState::default();
    let request = state.request_show().unwrap();

    state.cancel();

    assert!(!state.start(request, PaletteMainOrigin::HiddenApp));
    assert!(!state.active());
}

#[test]
fn stale_queued_dismissal_does_not_cancel_a_later_show() {
    let state = PalettePresentationState::default();
    let stale_hide = state.request_hide().unwrap();
    let show = state.request_show().unwrap();

    assert_eq!(state.complete(stale_hide), None);
    assert!(state.active());
    assert!(state.start(show, PaletteMainOrigin::ExternalApp));
}

#[test]
fn allow_main_window_clears_the_summon_guard() {
    let state = PalettePresentationState::default();
    let request = state.request_show().unwrap();
    assert!(state.start(request, PaletteMainOrigin::ExternalApp));
    assert!(!should_surface_main_on_reopen(state.active()));
    state.cancel();
    assert!(should_surface_main_on_reopen(state.active()));
}

#[test]
fn hidden_app_is_restored_as_app_hidden_after_palette_dismissal() {
    let origin = main_origin_before_palette(true, false);

    assert_eq!(origin, PaletteMainOrigin::HiddenApp);
    assert_eq!(
        dismissal_after_palette(origin),
        PaletteDismissal::RestoreHiddenApp
    );
}

#[test]
fn focused_main_window_regains_focus_after_palette_dismissal() {
    let origin = main_origin_before_palette(false, true);

    assert_eq!(origin, PaletteMainOrigin::FocusedMain);
    assert_eq!(
        dismissal_after_palette(origin),
        PaletteDismissal::RefocusMain
    );
}

#[test]
fn externally_summoned_palette_does_not_change_main_window_order() {
    let origin = main_origin_before_palette(false, false);

    assert_eq!(origin, PaletteMainOrigin::ExternalApp);
    assert_eq!(dismissal_after_palette(origin), PaletteDismissal::None);
}

#[test]
fn palette_moves_to_active_display_before_native_centering() {
    let actions = RefCell::new(Vec::new());

    position_on_active_screen(
        Some((-1920.0, 23.0)),
        |origin| actions.borrow_mut().push(format!("move:{origin:?}")),
        || actions.borrow_mut().push("center".to_string()),
    );

    assert_eq!(actions.into_inner(), ["move:(-1920.0, 23.0)", "center"]);
}

#[test]
fn palette_still_centers_when_the_active_display_is_unavailable() {
    let actions = RefCell::new(Vec::new());

    position_on_active_screen(
        None::<(f64, f64)>,
        |origin| actions.borrow_mut().push(format!("move:{origin:?}")),
        || actions.borrow_mut().push("center".to_string()),
    );

    assert_eq!(actions.into_inner(), ["center"]);
}

#[test]
fn hub_shortcut_keeps_toggle_behavior() {
    let state = PaletteShortcutState::default();
    let mut toggled = 0;
    let mut shown = false;

    dispatch_palette_shortcut(
        &state,
        PaletteLaunchMode::Hub,
        || toggled += 1,
        || shown = true,
    );

    assert_eq!(toggled, 1);
    assert!(!shown);
}

#[test]
fn taking_launch_mode_consumes_it_and_resets_to_hub() {
    let state = PaletteShortcutState::default();
    state.request_launch(PaletteLaunchMode::Skills);

    assert_eq!(state.take_launch_mode(), PaletteLaunchMode::Skills);
    assert_eq!(state.take_launch_mode(), PaletteLaunchMode::Hub);
}
