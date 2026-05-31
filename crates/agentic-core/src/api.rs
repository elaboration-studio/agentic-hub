//! High-level handlers that the Tauri shell wraps 1:1 as `#[tauri::command]`s.
//!
//! These are pure orchestration over the engine modules — no Tauri types — so
//! they stay unit-testable against a tempdir. The shell layer only does payload
//! marshalling and error mapping. See `docs/tech/modules/tauri-ipc-contract.md`.

use serde::{Deserialize, Serialize};

use crate::adapter_registry::{self, ResolvedAdapter};
use crate::model::{CapabilityItem, ScanResult, ToolCapabilityState, ToolId};
use crate::planner;
use crate::scanner;
use crate::settings::Settings;

/// Availability of one tool adapter, surfaced so the UI can disable a tool tab.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterStatus {
    pub tool: ToolId,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// Result of inspecting current per-tool state for a set of items.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub states: Vec<ToolCapabilityState>,
    pub adapter_statuses: Vec<AdapterStatus>,
}

/// Scan the source forest configured in settings (resolving the legacy single
/// root fallback).
pub fn scan(settings: &Settings) -> ScanResult {
    scanner::scan_all(&settings.resolve_sources())
}

/// Inspect current per-tool state for every enabled tool.
pub fn inspect(items: &[CapabilityItem], settings: &Settings) -> InspectResult {
    let mut states: Vec<ToolCapabilityState> = Vec::new();
    let mut adapter_statuses: Vec<AdapterStatus> = Vec::new();

    for tool in ToolId::ALL {
        let adapter = adapter_registry::resolve(settings, tool);
        match availability(&adapter) {
            Some(reason) => adapter_statuses.push(AdapterStatus {
                tool,
                available: false,
                unavailable_reason: Some(reason),
            }),
            None => {
                adapter_statuses.push(AdapterStatus {
                    tool,
                    available: true,
                    unavailable_reason: None,
                });
                states.extend(planner::inspect_tool(items, &adapter));
            }
        }
    }

    InspectResult {
        states,
        adapter_statuses,
    }
}

/// `None` if the adapter is available; `Some(reason)` otherwise.
fn availability(adapter: &ResolvedAdapter) -> Option<String> {
    if !adapter.enabled {
        return Some("Tool is disabled in settings".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn scan_uses_resolved_sources() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("skills/a/SKILL.md"), "# a");
        let settings = Settings {
            shared_root: dir.path().to_path_buf(),
            ..Settings::default()
        };
        let result = scan(&settings);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "skill:a");
        // Carries the synthesized Default source identity.
        assert_eq!(result.items[0].source_label, "Default");
    }

    #[test]
    fn inspect_reports_all_tools_and_skips_disabled() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("skills/a/SKILL.md"), "# a");
        let mut settings = Settings {
            shared_root: dir.path().to_path_buf(),
            ..Settings::default()
        };
        settings.tools.openclaw.enabled = false;

        let scanned = scan(&settings);
        let result = inspect(&scanned.items, &settings);

        // One status per tool.
        assert_eq!(result.adapter_statuses.len(), 4);
        let openclaw = result
            .adapter_statuses
            .iter()
            .find(|s| s.tool == ToolId::Openclaw)
            .unwrap();
        assert!(!openclaw.available);
        assert!(openclaw.unavailable_reason.is_some());

        // Disabled tool produces no states; enabled tools each inspect the skill.
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Openclaw));
        assert!(result
            .states
            .iter()
            .any(|s| s.tool == ToolId::Codex && s.item_id == "skill:a"));
    }
}
