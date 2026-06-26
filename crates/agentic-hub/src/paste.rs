//! Alfred-style paste into the frontmost app after the palette copies a command
//! body. macOS only: simulates Cmd+V via `enigo`, gated on Accessibility trust.
//! Non-macOS builds expose the same IPC surface but always no-op.

use serde::Serialize;

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteOutcome {
    pub pasted: bool,
    pub needs_permission: bool,
}

#[cfg(target_os = "macos")]
mod imp {
    use std::thread;
    use std::time::Duration;

    use enigo::{
        Direction::{Click, Press, Release},
        Enigo, Key, Keyboard, Settings,
    };
    use macos_accessibility_client::accessibility;
    use tauri::AppHandle;

    use super::PasteOutcome;
    use crate::palette;

    const PASTE_DELAY: Duration = Duration::from_millis(120);

    /// Whether Agentic Hub is trusted for Accessibility automation.
    pub fn accessibility_trusted(prompt: bool) -> bool {
        if prompt {
            accessibility::application_is_trusted_with_prompt()
        } else {
            accessibility::application_is_trusted()
        }
    }

    fn simulate_cmd_v() {
        let mut enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => e,
            Err(_) => return,
        };
        let _ = enigo.key(Key::Meta, Press);
        let _ = enigo.key(Key::Unicode('v'), Click);
        let _ = enigo.key(Key::Meta, Release);
    }

    /// Hide the palette, then post Cmd+V to the frontmost app after a short delay
    /// so focus can return from the non-activating panel.
    pub fn paste_to_frontmost(app: &AppHandle) -> PasteOutcome {
        let trusted = accessibility_trusted(false);
        if !trusted {
            let _ = accessibility_trusted(true);
            return PasteOutcome {
                pasted: false,
                needs_permission: true,
            };
        }

        palette::hide_palette(app);
        thread::spawn(|| {
            thread::sleep(PASTE_DELAY);
            simulate_cmd_v();
        });

        PasteOutcome {
            pasted: true,
            needs_permission: false,
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::AppHandle;

    use super::PasteOutcome;

    pub fn paste_to_frontmost(_app: &AppHandle) -> PasteOutcome {
        PasteOutcome {
            pasted: false,
            needs_permission: false,
        }
    }
}

pub use imp::paste_to_frontmost;
