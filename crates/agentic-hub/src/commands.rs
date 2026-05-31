//! Thin `#[tauri::command]` wrappers over `agentic_core`. No domain logic lives
//! here — only payload marshalling and error mapping. The read-only MVP surface:
//! settings load/save, scan, inspect, and source add/remove.

use agentic_core::api::{self, InspectResult};
use agentic_core::model::{CapabilityItem, ScanResult};
use agentic_core::paths::expand_tilde;
use agentic_core::settings::{Settings, SourceConfig, ToolsSettings};
use serde::Deserialize;

use crate::error::IpcError;

type IpcResult<T> = Result<T, IpcError>;

#[tauri::command]
pub async fn cmd_load_settings() -> IpcResult<Settings> {
    Ok(Settings::load()?)
}

#[tauri::command]
pub async fn cmd_save_settings(settings: Settings) -> IpcResult<()> {
    settings.save()?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ScanInput {
    pub sources: Vec<SourceConfig>,
}

#[tauri::command]
pub async fn cmd_scan(input: ScanInput) -> IpcResult<ScanResult> {
    // Reuse `resolve_sources` (slug assignment, tilde expansion, legacy
    // single-root fallback) by routing through a transient Settings.
    let settings = Settings {
        sources: input.sources,
        ..Settings::default()
    };
    Ok(api::scan(&settings))
}

#[tauri::command]
pub async fn cmd_inspect(
    items: Vec<CapabilityItem>,
    tools: ToolsSettings,
) -> IpcResult<InspectResult> {
    let settings = Settings {
        tools,
        ..Settings::default()
    };
    Ok(api::inspect(&items, &settings))
}

#[derive(Debug, Deserialize)]
pub struct AddSourceInput {
    pub label: String,
    pub path: String,
}

#[tauri::command]
pub async fn cmd_add_source(input: AddSourceInput) -> IpcResult<Settings> {
    let path = expand_tilde(&input.path);
    if !path.is_dir() {
        return Err(IpcError::new(
            "source_path_invalid",
            format!("Source path is not a directory: {}", path.display()),
        ));
    }
    let mut settings = Settings::load()?;
    settings.sources.push(SourceConfig {
        id: String::new(),
        label: input.label,
        path,
    });
    settings.save()?;
    Ok(settings)
}

#[derive(Debug, Deserialize)]
pub struct RemoveSourceInput {
    pub id: String,
}

#[tauri::command]
pub async fn cmd_remove_source(input: RemoveSourceInput) -> IpcResult<Settings> {
    let mut settings = Settings::load()?;
    // Persisted sources carry empty ids; resolved ids align positionally.
    let resolved = settings.resolve_sources();
    if let Some(pos) = resolved.iter().position(|s| s.id == input.id) {
        if pos < settings.sources.len() {
            settings.sources.remove(pos);
        }
    }
    settings.save()?;
    Ok(settings)
}
