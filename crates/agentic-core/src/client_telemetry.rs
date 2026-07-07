//! Rust-only Aptabase client identity and daily-active dedupe state.
//!
//! Persisted at `~/.agentic-hub/telemetry/client-state.json`. The WebView never
//! reads or writes this file — only the Tauri shell uses it when telemetry is on.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::managed_copy::now_iso8601;
use crate::paths::home_dir;

/// Anonymous installation identity and daily-active ping bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientTelemetryState {
    /// Stable per-installation id sent as `clientId` on `daily_active` events.
    #[serde(default)]
    pub installation_id: String,
    /// UTC calendar day (`YYYY-MM-DD`) of the last recorded `daily_active` ping.
    #[serde(default)]
    pub last_daily_active_date: Option<String>,
}

/// Canonical state path: `~/.agentic-hub/telemetry/client-state.json`.
pub fn default_path() -> PathBuf {
    home_dir()
        .join(".agentic-hub")
        .join("telemetry")
        .join("client-state.json")
}

pub struct ClientTelemetryStore {
    path: PathBuf,
}

impl Default for ClientTelemetryStore {
    fn default() -> Self {
        ClientTelemetryStore {
            path: default_path(),
        }
    }
}

impl ClientTelemetryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        ClientTelemetryStore { path: path.into() }
    }

    pub fn read(&self) -> Result<ClientTelemetryState> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|e| CoreError::StateParse(e.to_string())),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(ClientTelemetryState::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    pub fn write(&self, state: &ClientTelemetryState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(state)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

impl ClientTelemetryState {
    /// Returns the installation id, generating and persisting one when absent.
    pub fn ensure_installation_id(&mut self, store: &ClientTelemetryStore) -> Result<String> {
        if self.installation_id.is_empty() {
            self.installation_id = format!("client-{}", uuid::Uuid::new_v4());
            store.write(self)?;
        }
        Ok(self.installation_id.clone())
    }
}

/// Today's UTC calendar day as `YYYY-MM-DD`.
pub fn utc_date_yyyy_mm_dd() -> String {
    now_iso8601()[..10].to_string()
}

/// True when a `daily_active` ping has not yet been recorded for `today`.
pub fn should_record_daily_active(last: Option<&str>, today: &str) -> bool {
    match last {
        None => true,
        Some(d) => d != today,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_record_daily_active_only_once_per_day() {
        assert!(should_record_daily_active(None, "2026-07-07"));
        assert!(!should_record_daily_active(
            Some("2026-07-07"),
            "2026-07-07"
        ));
        assert!(should_record_daily_active(
            Some("2026-07-06"),
            "2026-07-07"
        ));
    }

    #[test]
    fn client_state_roundtrips_and_generates_installation_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client-state.json");
        let store = ClientTelemetryStore::with_path(&path);
        let mut state = ClientTelemetryState::default();

        let id = state.ensure_installation_id(&store).unwrap();
        assert!(id.starts_with("client-"));
        assert_eq!(state.installation_id, id);

        let reloaded = store.read().unwrap();
        assert_eq!(reloaded.installation_id, id);
        assert_eq!(reloaded.last_daily_active_date, None);
    }

    #[test]
    fn utc_date_yyyy_mm_dd_matches_iso_prefix() {
        let date = utc_date_yyyy_mm_dd();
        assert_eq!(date.len(), 10);
        assert!(date.as_bytes().get(4) == Some(&b'-'));
        assert!(date.as_bytes().get(7) == Some(&b'-'));
    }
}
