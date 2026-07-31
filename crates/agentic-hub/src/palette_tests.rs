use std::collections::HashSet;

use agentic_core::settings::{PaletteLaunchMode, Settings};

use super::{
    dispatch_palette_shortcut, replace_registered_shortcuts,
    replace_registered_shortcuts_and_persist, PaletteShortcutSet, PaletteShortcutState,
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
