//! Durable spool for hook payloads that fail to reach the live collector.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use agentic_core::paths::home_dir;
use agentic_core::settings::Settings;
use agentic_core::UsageStore;
use serde_json::Value;

use crate::usage_attribution::{is_traced_source_tool, AttributionState};
use crate::usage_catalog::attribute_batch;

const SPOOL_DIR_NAME: &str = "spool";

pub fn usage_spool_dir() -> PathBuf {
    home_dir()
        .join(".agentic-hub")
        .join("usage")
        .join(SPOOL_DIR_NAME)
}

/// Ensure the spool directory exists. Used by tests and mirrored by the hook script.
pub fn ensure_spool_dir() -> io::Result<PathBuf> {
    let dir = usage_spool_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Persist a failed hook delivery for later drain.
///
/// Layout per event (also written by `usage-tracer.sh` on curl failure):
/// - `{id}.meta` — `token\nsource_tool\n`
/// - `{id}.body` — raw JSON payload bytes
#[cfg_attr(not(test), allow(dead_code))]
pub fn enqueue_failed_delivery(
    token: &str,
    source_tool: &str,
    payload: &[u8],
) -> io::Result<PathBuf> {
    let dir = ensure_spool_dir()?;
    let id = unique_spool_id(source_tool);
    let meta_path = dir.join(format!("{id}.meta"));
    let body_path = dir.join(format!("{id}.body"));
    let meta = format!("{token}\n{source_tool}\n");
    fs::write(&meta_path, meta)?;
    if let Err(error) = fs::write(&body_path, payload) {
        let _ = fs::remove_file(&meta_path);
        return Err(error);
    }
    Ok(body_path)
}

pub fn drain_spool(settings: &Settings, attribution: &AttributionState) -> Result<u32, String> {
    drain_spool_into(settings, attribution, &UsageStore::new())
}

fn drain_spool_into(
    settings: &Settings,
    attribution: &AttributionState,
    store: &UsageStore,
) -> Result<u32, String> {
    let dir = match ensure_spool_dir() {
        Ok(dir) => dir,
        Err(_) => return Ok(0),
    };
    if !dir.is_dir() {
        return Ok(0);
    }
    let expected_token = settings.usage_tracing.collector_token.as_str();
    let mut drained = 0_u32;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(0);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if extension != "body" {
            continue;
        }
        let meta_path = path.with_extension("meta");
        match process_spool_entry(
            &path,
            &meta_path,
            expected_token,
            settings,
            attribution,
            store,
        ) {
            Ok(true) => drained = drained.saturating_add(1),
            Ok(false) => {}
            Err(_) => {
                // Leave the pair for a later attempt when persistence fails.
            }
        }
    }
    Ok(drained)
}

fn process_spool_entry(
    body_path: &Path,
    meta_path: &Path,
    expected_token: &str,
    settings: &Settings,
    attribution: &AttributionState,
    store: &UsageStore,
) -> Result<bool, String> {
    let meta = fs::read_to_string(meta_path).map_err(|error| error.to_string())?;
    let mut lines = meta.lines();
    let token = lines.next().unwrap_or_default();
    let source_tool = lines.next().unwrap_or_default();
    if token != expected_token {
        // Stale after token rotation — discard without persisting.
        let _ = fs::remove_file(body_path);
        let _ = fs::remove_file(meta_path);
        return Ok(false);
    }
    if !is_traced_source_tool(source_tool) {
        let _ = fs::remove_file(body_path);
        let _ = fs::remove_file(meta_path);
        return Ok(false);
    }
    let body = fs::read(body_path).map_err(|error| error.to_string())?;
    let raw: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    let batch = attribution.normalize(&raw, source_tool);
    let attributed = attribute_batch(batch, settings, source_tool);
    store
        .insert_events(&attributed.events, &attributed.catalog_items)
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_file(body_path);
    let _ = fs::remove_file(meta_path);
    Ok(true)
}

#[cfg_attr(not(test), allow(dead_code))]
fn unique_spool_id(source_tool: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{source_tool}-{nanos}-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::settings::Settings;
    use serde_json::json;
    use std::sync::Mutex;

    static SPOOL_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_isolated_spool<T>(f: impl FnOnce(&Path) -> T) -> T {
        let _guard = SPOOL_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = usage_spool_dir();
        let backup = dir.with_extension("bak-test");
        let _ = fs::remove_dir_all(&backup);
        if dir.exists() {
            let _ = fs::rename(&dir, &backup);
        }
        let _ = fs::remove_dir_all(&dir);
        let result = f(&dir);
        let _ = fs::remove_dir_all(&dir);
        if backup.exists() {
            let _ = fs::rename(&backup, &dir);
        }
        result
    }

    #[test]
    fn enqueue_writes_meta_and_body_pair() {
        with_isolated_spool(|_| {
            let payload = br#"{"event_type":"beforeSubmitPrompt","prompt":"/health"}"#;
            let body_path = enqueue_failed_delivery("token-a", "cursor", payload).expect("enqueue");
            let meta_path = body_path.with_extension("meta");
            assert!(body_path.exists());
            assert_eq!(fs::read(&body_path).unwrap(), payload);
            assert_eq!(fs::read_to_string(&meta_path).unwrap(), "token-a\ncursor\n");
        });
    }

    #[test]
    fn drain_persists_matching_token_and_discards_stale() {
        with_isolated_spool(|_| {
            let mut settings = Settings::default();
            settings.usage_tracing.collector_token = "live-token".to_string();
            let attribution = AttributionState::default();
            let db_dir =
                std::env::temp_dir().join(format!("agentic-hub-spool-db-{}", std::process::id()));
            let _ = fs::create_dir_all(&db_dir);
            let store = UsageStore::with_path(db_dir.join("trace.db"));
            store.ensure_ready().unwrap();

            let good = json!({
                "event_type": "postToolUse",
                "tool_name": "Skill",
                "tool_input": { "skill": "spool-drain-skill" }
            });
            let live_body = enqueue_failed_delivery(
                "live-token",
                "cursor",
                serde_json::to_vec(&good).unwrap().as_slice(),
            )
            .unwrap();
            let stale_body = enqueue_failed_delivery(
                "stale-token",
                "cursor",
                serde_json::to_vec(&good).unwrap().as_slice(),
            )
            .unwrap();

            let drained = drain_spool_into(&settings, &attribution, &store).expect("drain");
            assert_eq!(drained, 1);
            // Concurrent writers can land other files in the shared spool dir;
            // only the pair this test enqueued must be gone.
            assert!(!live_body.exists());
            assert!(!live_body.with_extension("meta").exists());
            assert!(!stale_body.exists());
            assert!(!stale_body.with_extension("meta").exists());
            assert_eq!(store.event_count().unwrap(), 1);
            let _ = fs::remove_dir_all(&db_dir);
        });
    }
}
