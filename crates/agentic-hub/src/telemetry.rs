//! Opt-in anonymous usage telemetry (Aptabase).
//!
//! Telemetry is off by default and gated on explicit user consent
//! (`Settings.telemetry.enabled`). The Aptabase plugin is always registered —
//! it sends nothing until `track_event` is called — and every call is guarded
//! by the [`TelemetryState`] flag, so a runtime toggle takes effect at once and
//! nothing leaves the machine while disabled. Events are sent from Rust only;
//! the WebView never calls out.

use std::sync::atomic::{AtomicBool, Ordering};

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
