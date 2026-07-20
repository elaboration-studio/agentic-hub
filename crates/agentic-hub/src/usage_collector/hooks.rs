use super::*;

pub fn sync_tracer_hooks(settings: &Settings) -> Result<(), String> {
    ensure_tracer_script(settings)?;
    for tool in USAGE_TRACER_TOOLS {
        let enabled = usage_tracer_enabled(settings, tool);
        sync_tracer_hook_for_tool(settings, tool, enabled)?;
    }
    Ok(())
}

pub fn synced_tracer_tools(settings: &Settings) -> Vec<ToolId> {
    USAGE_TRACER_TOOLS
        .iter()
        .copied()
        .filter(|tool| usage_tracer_enabled(settings, *tool))
        .collect()
}

pub(super) fn tracer_hook_installed(settings: &Settings, tool: ToolId) -> bool {
    let adapter = adapter_registry::resolve(settings, tool);
    let Some(path) = adapter.hooks_file else {
        return false;
    };
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    contains_hook_marker(&value, &usage_tracer_hook_id(tool))
}

fn contains_hook_marker(value: &Value, hook_id: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map
                .get("_agenticHub")
                .and_then(Value::as_object)
                .and_then(|marker| marker.get("hookId"))
                .and_then(Value::as_str)
                == Some(hook_id)
            {
                return true;
            }
            map.values()
                .any(|child| contains_hook_marker(child, hook_id))
        }
        Value::Array(values) => values
            .iter()
            .any(|child| contains_hook_marker(child, hook_id)),
        _ => false,
    }
}

fn sync_tracer_hook_for_tool(
    settings: &Settings,
    tool: ToolId,
    enabled: bool,
) -> Result<(), String> {
    let hook_dir = usage_tracer_hook_dir(tool);
    fs::create_dir_all(&hook_dir).map_err(|e| e.to_string())?;
    let manifest = usage_tracer_manifest(settings, tool, &hook_dir);
    write_tracer_manifest(&hook_dir, &manifest)?;
    let item = usage_tracer_item(tool, &hook_dir);
    let adapter = adapter_registry::resolve(settings, tool);
    hook_sync::sync_single_json_hook(&adapter, &item, &manifest, enabled)
        .map(|_| ())
        .map_err(|e| e.message)
}

fn ensure_tracer_script(_settings: &Settings) -> Result<(), String> {
    let path = usage_tracer_root().join(USAGE_TRACER_SCRIPT);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Buffer stdin, POST to the collector, and on failure spool for later drain.
    // Always exit 0 so agentic tools are never blocked by tracing.
    // Spool layout MUST match `spool.rs`: `{id}.meta` = `token\nsource_tool\n`,
    // `{id}.body` = raw JSON payload bytes under `~/.agentic-hub/usage/spool/`.
    fs::write(
        &path,
        r#"#!/bin/sh
url="$1"
token="$2"
source_tool="$3"
payload=$(cat)
if printf '%s' "$payload" | /usr/bin/curl -fsS --max-time 1.0 \
  -H "content-type: application/json" \
  -H "x-agentic-hub-token: ${token}" \
  -H "x-agentic-hub-source-tool: ${source_tool}" \
  --data-binary @- "${url}" >/dev/null 2>&1; then
  exit 0
fi
spool_dir="${HOME}/.agentic-hub/usage/spool"
mkdir -p "${spool_dir}" 2>/dev/null || exit 0
id="${source_tool}-$(date +%s)-$$"
printf '%s\n%s\n' "$token" "$source_tool" > "${spool_dir}/${id}.meta" 2>/dev/null || exit 0
printf '%s' "$payload" > "${spool_dir}/${id}.body" 2>/dev/null || rm -f "${spool_dir}/${id}.meta"
exit 0
"#,
    )
    .map_err(|e| e.to_string())?;
    set_executable(&path).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub(super) fn write_tracer_manifest(
    hook_dir: &Path,
    manifest: &HookManifest,
) -> Result<(), String> {
    let events: Vec<Value> = manifest
        .events
        .iter()
        .map(|event| json!({ "name": hook_event_name(event.name), "matcher": event.matcher.as_deref() }))
        .collect();
    let body = json!({
        "$schema": "agentic-hub.hook.v1",
        "id": &manifest.id,
        "name": manifest.name.as_deref(),
        "description": manifest.description.as_deref(),
        "events": events,
        "command": &manifest.command,
        "timeout": manifest.timeout,
        "targets": manifest.effective_targets().iter().map(|tool| tool.as_str()).collect::<Vec<_>>()
    });
    fs::write(
        hook_dir.join("hook.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        ),
    )
    .map_err(|e| e.to_string())
}

fn hook_event_name(event: HookCanonicalEvent) -> &'static str {
    match event {
        HookCanonicalEvent::PreToolUse => "PreToolUse",
        HookCanonicalEvent::PostToolUse => "PostToolUse",
        HookCanonicalEvent::PostToolUseFailure => "PostToolUseFailure",
        HookCanonicalEvent::UserPromptSubmit => "UserPromptSubmit",
        HookCanonicalEvent::UserPromptExpansion => "UserPromptExpansion",
        HookCanonicalEvent::Stop => "Stop",
        HookCanonicalEvent::SessionStart => "SessionStart",
        HookCanonicalEvent::SessionEnd => "SessionEnd",
        HookCanonicalEvent::PreCompact => "PreCompact",
        HookCanonicalEvent::PostCompact => "PostCompact",
        HookCanonicalEvent::Notification => "Notification",
        HookCanonicalEvent::PermissionRequest => "PermissionRequest",
    }
}
