use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Direct command-palette search accelerators persisted alongside the hub
/// toggle shortcut. These strings are parsed by the Tauri global-shortcut
/// plugin before they are registered with the operating system.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteQuickSearchShortcuts {
    pub all_resources: String,
    pub skills: String,
    pub commands: String,
}

impl Default for PaletteQuickSearchShortcuts {
    fn default() -> Self {
        Self {
            all_resources: "Cmd+Alt+Ctrl+A".to_string(),
            skills: "Cmd+Alt+Ctrl+S".to_string(),
            commands: "Cmd+Alt+Ctrl+C".to_string(),
        }
    }
}

/// One-shot destination requested by a global palette accelerator.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaletteLaunchMode {
    #[default]
    Hub,
    AllResources,
    Skills,
    Commands,
}

/// User-facing validation failure for the complete palette accelerator set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteShortcutValidationError {
    Invalid(String),
    Duplicate(String),
}

/// Default global accelerator for the command palette.
pub fn default_palette_shortcut() -> String {
    "Cmd+Alt+A".to_string()
}

/// Lightweight sanity check before the Tauri plugin performs the authoritative
/// parse at the trusted OS-registration boundary.
pub fn is_valid_shortcut(accelerator: &str) -> bool {
    const MODIFIERS: [&str; 11] = [
        "cmd",
        "command",
        "ctrl",
        "control",
        "alt",
        "option",
        "shift",
        "super",
        "meta",
        "cmdorctrl",
        "commandorcontrol",
    ];
    let parts: Vec<&str> = accelerator
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return false;
    }
    parts
        .iter()
        .any(|part| !MODIFIERS.contains(&part.to_ascii_lowercase().as_str()))
}

/// Validate the four persisted palette accelerators before any settings write.
pub fn validate_palette_shortcuts(
    hub: &str,
    quick: &PaletteQuickSearchShortcuts,
) -> Result<(), PaletteShortcutValidationError> {
    let shortcuts = [hub, &quick.all_resources, &quick.skills, &quick.commands];
    let mut seen = HashSet::new();
    for shortcut in shortcuts {
        if !is_valid_shortcut(shortcut) {
            return Err(PaletteShortcutValidationError::Invalid(
                shortcut.to_string(),
            ));
        }
        if !seen.insert(shortcut.trim().to_ascii_lowercase()) {
            return Err(PaletteShortcutValidationError::Duplicate(
                shortcut.to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    use super::*;

    #[test]
    fn palette_shortcut_defaults_to_cmd_alt_a() {
        assert_eq!(Settings::default().palette_shortcut, "Cmd+Alt+A");
    }

    #[test]
    fn palette_quick_search_shortcuts_have_stable_defaults() {
        assert_eq!(
            Settings::default().palette_quick_search_shortcuts,
            PaletteQuickSearchShortcuts {
                all_resources: "Cmd+Alt+Ctrl+A".to_string(),
                skills: "Cmd+Alt+Ctrl+S".to_string(),
                commands: "Cmd+Alt+Ctrl+C".to_string(),
            }
        );
    }

    #[test]
    fn legacy_config_without_quick_search_shortcuts_uses_defaults() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("paletteQuickSearchShortcuts");

        let loaded: Settings = serde_json::from_value(value).unwrap();

        assert_eq!(
            loaded.palette_quick_search_shortcuts,
            PaletteQuickSearchShortcuts::default()
        );
    }

    #[test]
    fn validates_palette_shortcut_shape() {
        assert!(is_valid_shortcut("Cmd+Alt+A"));
        assert!(is_valid_shortcut("CmdOrCtrl+Shift+K"));
        assert!(is_valid_shortcut("Space"));
        assert!(!is_valid_shortcut("Cmd+Alt"));
        assert!(!is_valid_shortcut(""));
        assert!(!is_valid_shortcut("   "));
    }

    #[test]
    fn palette_shortcut_set_rejects_a_malformed_direct_shortcut() {
        let mut settings = Settings::default();
        settings.palette_quick_search_shortcuts.skills = "Cmd+Alt".to_string();

        let error = validate_palette_shortcuts(
            &settings.palette_shortcut,
            &settings.palette_quick_search_shortcuts,
        )
        .unwrap_err();

        assert_eq!(
            error,
            PaletteShortcutValidationError::Invalid("Cmd+Alt".to_string())
        );
    }

    #[test]
    fn palette_shortcut_set_rejects_duplicates_across_all_four_shortcuts() {
        let mut settings = Settings::default();
        settings.palette_quick_search_shortcuts.commands = settings.palette_shortcut.clone();

        let error = validate_palette_shortcuts(
            &settings.palette_shortcut,
            &settings.palette_quick_search_shortcuts,
        )
        .unwrap_err();

        assert_eq!(
            error,
            PaletteShortcutValidationError::Duplicate("Cmd+Alt+A".to_string())
        );
    }
}
