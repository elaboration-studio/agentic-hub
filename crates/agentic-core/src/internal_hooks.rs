//! Settings-managed hooks owned by Agentic Hub itself.

use std::path::{Path, PathBuf};

use crate::hook_sync::{HookCanonicalEvent, HookEventSpec, HookManifest};
use crate::model::{CapabilityItem, CapabilityKind, SourceRef, ToolId};
use crate::paths::home_dir;
use crate::settings::Settings;

pub const SOURCE_ID: &str = "agentic-hub";
pub const SOURCE_LABEL: &str = "Agentic Hub";

pub const USAGE_TRACER_ID_PREFIX: &str = "agentic-hub-usage-tracer";
pub const USAGE_TRACER_SCRIPT: &str = "usage-tracer.sh";

pub const USAGE_TRACER_TOOLS: [ToolId; 5] = [
    ToolId::Codex,
    ToolId::Claude,
    ToolId::Cursor,
    ToolId::Kiro,
    ToolId::Grok,
];

pub fn is_internal_item(item: &CapabilityItem) -> bool {
    item.source_id == SOURCE_ID
}

pub fn items(settings: &Settings) -> Vec<CapabilityItem> {
    USAGE_TRACER_TOOLS
        .iter()
        .copied()
        .filter(|tool| settings.usage_tracing.enabled && settings.tools.for_tool(*tool).enabled)
        .map(|tool| usage_tracer_item(tool, &usage_tracer_hook_dir(tool)))
        .collect()
}

pub fn append_items(existing_items: &mut Vec<CapabilityItem>, settings: &Settings) {
    for item in items(settings) {
        if !existing_items.iter().any(|existing| existing.id == item.id) {
            existing_items.push(item);
        }
    }
}

pub fn insert_manifests(
    manifests: &mut std::collections::HashMap<String, HookManifest>,
    settings: &Settings,
) {
    for tool in USAGE_TRACER_TOOLS {
        if !settings.usage_tracing.enabled || !settings.tools.for_tool(tool).enabled {
            continue;
        }
        let hook_dir = usage_tracer_hook_dir(tool);
        manifests.insert(
            usage_tracer_item_id(tool),
            usage_tracer_manifest(settings, tool, &hook_dir),
        );
    }
}

pub fn item_enabled_for_tool(settings: &Settings, item: &CapabilityItem, tool: ToolId) -> bool {
    if !is_internal_item(item) {
        return false;
    }
    item.id == usage_tracer_item_id(tool) && usage_tracer_enabled(settings, tool)
}

pub fn usage_tracer_enabled(settings: &Settings, tool: ToolId) -> bool {
    if !settings.usage_tracing.enabled || !settings.tools.for_tool(tool).enabled {
        return false;
    }
    // Kiro joined after capture_tools shipped as [codex, claude, cursor].
    // Existing configs omit it; an enabled Kiro tool is still captured.
    settings.usage_tracing.capture_tools.contains(&tool)
        || tool == ToolId::Kiro
        || tool == ToolId::Grok
}

pub fn usage_tracer_item_id(tool: ToolId) -> String {
    format!("hook:{}", usage_tracer_hook_id(tool))
}

pub fn usage_tracer_hook_id(tool: ToolId) -> String {
    format!("{USAGE_TRACER_ID_PREFIX}-{}", tool.as_str())
}

pub fn usage_tracer_hook_dir(tool: ToolId) -> PathBuf {
    usage_tracer_root().join(usage_tracer_hook_id(tool))
}

pub fn usage_tracer_root() -> PathBuf {
    home_dir().join(".agentic-hub").join("hooks")
}

pub fn usage_tracer_item(tool: ToolId, hook_dir: &Path) -> CapabilityItem {
    let hook_id = usage_tracer_hook_id(tool);
    CapabilityItem {
        id: format!("hook:{hook_id}"),
        kind: CapabilityKind::Hook,
        name: hook_id.clone(),
        source_path: hook_dir.to_path_buf(),
        relative_path: PathBuf::from(hook_id),
        source_id: SOURCE_ID.to_string(),
        source_label: SOURCE_LABEL.to_string(),
        source: SourceRef {
            rel_home: "~/.agentic-hub".to_string(),
            folder: ".agentic-hub".to_string(),
        },
        valid: true,
        validation_errors: Vec::new(),
    }
}

pub fn usage_tracer_manifest(settings: &Settings, tool: ToolId, hook_dir: &Path) -> HookManifest {
    HookManifest {
        id: usage_tracer_hook_id(tool),
        name: Some("Agentic Hub Usage Tracer".to_string()),
        description: Some(
            "Forward terminal hook events to Agentic Hub's local usage collector.".to_string(),
        ),
        events: usage_tracer_events(tool),
        command: usage_tracer_command(settings, tool, hook_dir),
        timeout: Some(2),
        loop_limit: None,
        targets: Some(vec![tool]),
    }
}

pub fn usage_tracer_events(tool: ToolId) -> Vec<HookEventSpec> {
    let mut events = Vec::new();
    if prompt_skill_attribution_tool(tool) {
        events.push(HookEventSpec {
            name: HookCanonicalEvent::UserPromptSubmit,
            matcher: Some(".*".to_string()),
        });
    }
    if tool == ToolId::Claude {
        events.push(HookEventSpec {
            name: HookCanonicalEvent::UserPromptExpansion,
            matcher: Some(".*".to_string()),
        });
    }
    events.push(HookEventSpec {
        name: HookCanonicalEvent::PostToolUse,
        matcher: Some(".*".to_string()),
    });
    events.push(HookEventSpec {
        name: HookCanonicalEvent::PostToolUseFailure,
        matcher: Some(".*".to_string()),
    });
    events
}

pub fn usage_tracer_command(settings: &Settings, tool: ToolId, hook_dir: &Path) -> String {
    let script = hook_dir
        .parent()
        .map(|parent| parent.join(USAGE_TRACER_SCRIPT))
        .unwrap_or_else(|| hook_dir.join(USAGE_TRACER_SCRIPT));
    format!(
        "/bin/sh '{}' 'http://127.0.0.1:{}/events' '{}' '{}'",
        shell_escape_path(&script),
        settings.usage_tracing.collector_port,
        shell_escape(&settings.usage_tracing.collector_token),
        tool.as_str()
    )
}

fn prompt_skill_attribution_tool(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::Codex | ToolId::Claude | ToolId::Cursor | ToolId::Kiro | ToolId::Grok
    )
}

fn shell_escape_path(path: &Path) -> String {
    shell_escape(&path.to_string_lossy())
}

fn shell_escape(value: &str) -> String {
    value.replace('\'', "'\\''")
}
