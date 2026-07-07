//! Anonymous usage telemetry (Aptabase).
//!
//! Telemetry is on by default and gated on `Settings.telemetry.enabled` (the
//! user can disable it in Config). The Aptabase plugin is always registered —
//! it sends nothing until `track_event` is called — and every call is guarded
//! by the [`TelemetryState`] flag, so a runtime toggle takes effect at once and
//! nothing leaves the machine while disabled. Events are sent from Rust only;
//! the WebView never calls out.

use std::sync::atomic::{AtomicBool, Ordering};

use agentic_core::{
    should_record_daily_active, utc_date_yyyy_mm_dd, ClientTelemetryStore,
};
use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_aptabase::EventTracker;

/// Aptabase App Key. Not a secret — it ships in every binary regardless, so a
/// `.env`/`dotenvy` indirection would add no real protection.
pub const APTABASE_KEY: &str = "A-US-7480293241";

/// Live consent flag, mirrored from `Settings.telemetry.enabled`. Seeded at
/// startup and updated whenever settings are saved.
#[derive(Default)]
pub struct TelemetryState(AtomicBool);

impl TelemetryState {
    pub fn set_enabled(&self, enabled: bool) {
        self.0.store(enabled, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Record one `daily_active` ping per UTC calendar day when the user engages with
/// the app (focus, palette summon, dock reopen). `app_started` alone does not
/// count — background launches stay silent until the user interacts.
pub fn record_client_engagement(app: &AppHandle) {
    if !app.state::<TelemetryState>().enabled() {
        return;
    }

    let store = ClientTelemetryStore::new();
    let Ok(mut state) = store.read() else {
        return;
    };
    let today = utc_date_yyyy_mm_dd();
    if !should_record_daily_active(state.last_daily_active_date.as_deref(), &today) {
        return;
    }

    let Ok(client_id) = state.ensure_installation_id(&store) else {
        return;
    };
    let props = json!({ "clientId": client_id });
    if app.track_event("daily_active", Some(props)).is_err() {
        return;
    }

    state.last_daily_active_date = Some(today);
    let _ = store.write(&state);
}

#[cfg(test)]
mod tests {
    use agentic_core::ClientTelemetryState;

    use super::*;

    #[test]
    fn engagement_state_advances_only_when_date_changes() {
        let mut state = ClientTelemetryState {
            last_daily_active_date: Some("2026-07-06".to_string()),
            ..ClientTelemetryState::default()
        };
        assert!(should_record_daily_active(
            state.last_daily_active_date.as_deref(),
            "2026-07-07"
        ));
        state.last_daily_active_date = Some("2026-07-07".to_string());
        assert!(!should_record_daily_active(
            state.last_daily_active_date.as_deref(),
            "2026-07-07"
        ));
    }
}
