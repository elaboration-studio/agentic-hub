//! Grok Build hook projection — one Claude-style JSON file per hook id
//! under `~/.grok/hooks/`. See `docs/tech/modules/grok-tool-adapter.md`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::adapter_registry::ResolvedAdapter;
use crate::hook_sync::{self, HookCanonicalEvent, HookEventSpec, HookManifest};
use crate::model::{
    CapabilityItem, CapabilityKind, HookSyncError, HookSyncOutcome, LinkState, ToolCapabilityState,
    ToolId,
};

fn grok_event_key(event: HookCanonicalEvent) -> Option<&'static str> {
    use HookCanonicalEvent::{
        Notification, PermissionRequest, PostCompact, PostToolUse, PostToolUseFailure, PreCompact,
        PreToolUse, SessionEnd, SessionStart, Stop, UserPromptExpansion, UserPromptSubmit,
    };
    match event {
        PreToolUse | PostToolUse | PostToolUseFailure | UserPromptSubmit | Stop | SessionStart
        | SessionEnd | PreCompact | PostCompact | Notification => Some(event.pascal()),
        UserPromptExpansion | PermissionRequest => None,
    }
}

fn hook_target(adapter: &ResolvedAdapter, hook_id: &str) -> Option<PathBuf> {
    let dir = adapter.hooks_dir.as_ref()?;
    if !adapter.hooks_enabled {
        return None;
    }
    Some(dir.join(format!("{hook_id}.json")))
}

fn marker(id: &str, hash: &str) -> Value {
    json!({ "hookId": id, "sourceHash": hash, "version": 1 })
}

fn group_entry(item: &CapabilityItem, m: &HookManifest, hash: &str, spec: &HookEventSpec) -> Value {
    let mut inner = Map::new();
    inner.insert("type".to_string(), json!("command"));
    inner.insert(
        "command".to_string(),
        Value::String(hook_sync::expand_hook_dir(&m.command, &item.source_path)),
    );
    if let Some(t) = m.timeout {
        inner.insert("timeout".to_string(), json!(t));
    }
    let mut g = Map::new();
    if let Some(matcher) = &spec.matcher {
        g.insert("matcher".to_string(), Value::String(matcher.clone()));
    }
    g.insert(
        "hooks".to_string(),
        Value::Array(vec![Value::Object(inner)]),
    );
    g.insert("_agenticHub".to_string(), marker(&m.id, hash));
    Value::Object(g)
}

fn build_hooks_object(
    item: &CapabilityItem,
    m: &HookManifest,
    hash: &str,
    notes: &mut Vec<String>,
) -> Map<String, Value> {
    let mut hooks = Map::new();
    for spec in &m.events {
        match grok_event_key(spec.name) {
            Some(key) => {
                hooks
                    .entry(key.to_string())
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .expect("event bucket is an array")
                    .push(group_entry(item, m, hash, spec));
            }
            None => notes.push(format!(
                "{} is not supported by Grok; entry skipped.",
                spec.name.pascal()
            )),
        }
    }
    hooks
}

enum FileRead {
    Missing,
    Foreign,
    Broken,
    ForeignContent,
    Ok(Map<String, Value>),
}

fn read_managed_hooks(path: &Path, hook_id: &str) -> FileRead {
    match fs::symlink_metadata(path) {
        Err(_) => FileRead::Missing,
        Ok(meta) if !meta.is_file() => FileRead::Foreign,
        Ok(_) => match fs::read_to_string(path) {
            Err(_) => FileRead::Broken,
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(Value::Object(root)) => {
                    let Some(hooks_val) = root.get("hooks") else {
                        return FileRead::Broken;
                    };
                    let hooks_obj = match hooks_val {
                        Value::Object(map) => map.clone(),
                        _ => return FileRead::Broken,
                    };
                    if hooks_obj.is_empty() {
                        return FileRead::Ok(Map::new());
                    }
                    let mut managed = Map::new();
                    for (event, arr) in hooks_obj {
                        let Some(items) = arr.as_array() else {
                            return FileRead::ForeignContent;
                        };
                        let ours: Vec<Value> = items
                            .iter()
                            .filter(|e| {
                                e.get("_agenticHub")
                                    .and_then(|a| a.get("hookId"))
                                    .and_then(Value::as_str)
                                    == Some(hook_id)
                            })
                            .cloned()
                            .collect();
                        if !ours.is_empty() {
                            if ours.len() != items.len() {
                                return FileRead::ForeignContent;
                            }
                            managed.insert(event, Value::Array(ours));
                        } else if !items.is_empty() {
                            return FileRead::ForeignContent;
                        }
                    }
                    FileRead::Ok(managed)
                }
                Ok(_) => FileRead::Broken,
                Err(_) => FileRead::Broken,
            },
        },
    }
}

fn managed_hash(hooks: &Map<String, Value>, hook_id: &str) -> Option<String> {
    for arr in hooks.values() {
        for entry in arr.as_array()? {
            let agentic = entry.get("_agenticHub")?;
            if agentic.get("hookId").and_then(Value::as_str) == Some(hook_id) {
                return agentic
                    .get("sourceHash")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
    }
    None
}

/// Per-(hook, tool) state for Grok hook files.
pub fn inspect_grok_hooks(
    items: &[CapabilityItem],
    manifests: &HashMap<String, HookManifest>,
    adapter: &ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    let mut out = Vec::new();
    if adapter.tool_id != ToolId::Grok || !adapter.hooks_enabled {
        return out;
    }
    if adapter.hooks_dir.is_none() {
        return out;
    }

    for item in items.iter().filter(|i| i.kind == CapabilityKind::Hook) {
        let Some(m) = manifests.get(&item.id) else {
            continue;
        };
        if !m.effective_targets().contains(&ToolId::Grok) {
            continue;
        }
        let Some(target) = hook_target(adapter, &m.id) else {
            continue;
        };
        let hash = hook_sync::projection_source_hash(item, m);
        let state = match read_managed_hooks(&target, &m.id) {
            FileRead::Missing => LinkState::Disabled,
            FileRead::Foreign | FileRead::ForeignContent => LinkState::ForeignFile,
            FileRead::Broken => LinkState::Broken,
            FileRead::Ok(hooks) if hooks.is_empty() => LinkState::Disabled,
            FileRead::Ok(hooks) => match managed_hash(&hooks, &m.id) {
                Some(h) if h == hash => LinkState::Enabled,
                Some(_) => LinkState::Stale,
                None => LinkState::Disabled,
            },
        };
        out.push(ToolCapabilityState {
            tool: adapter.tool_id,
            item_id: item.id.clone(),
            target_path: target.clone(),
            state,
            current_link_target: Some(target),
        });
    }
    out
}

fn atomic_write_json(target: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!("{}\n", serde_json::to_string_pretty(value)?);
    let tmp = target.with_extension("agentic-grok-hooks.tmp");
    if fs::write(&tmp, &body).is_ok() && fs::rename(&tmp, target).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&tmp);
    fs::write(target, body)
}

fn io_err(path: &Path, e: &std::io::Error) -> HookSyncError {
    HookSyncError {
        path: path.to_path_buf(),
        code: if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission_denied".to_string()
        } else {
            "io_error".to_string()
        },
        message: e.to_string(),
    }
}

fn refuse_unmanaged_target(path: &Path, state: FileRead) -> Result<(), HookSyncError> {
    match state {
        FileRead::Missing => Ok(()),
        FileRead::Ok(hooks) if !hooks.is_empty() => Ok(()),
        FileRead::Foreign | FileRead::ForeignContent | FileRead::Ok(_) => Err(HookSyncError {
            path: path.to_path_buf(),
            code: "conflict_real_file_at_target".to_string(),
            message: "Hook config file is not fully managed; refusing to overwrite".to_string(),
        }),
        FileRead::Broken => Err(HookSyncError {
            path: path.to_path_buf(),
            code: "hook_target_broken_json".to_string(),
            message: "Hook config file is not valid Grok hook JSON; refusing to overwrite"
                .to_string(),
        }),
    }
}

/// Write or remove Grok hook JSON files for enabled hooks targeting Grok.
pub fn sync_grok_hooks(
    adapter: &ResolvedAdapter,
    enabled: &[(&CapabilityItem, &HookManifest)],
) -> Result<(HookSyncOutcome, Vec<String>), HookSyncError> {
    if adapter.tool_id != ToolId::Grok {
        return Ok((HookSyncOutcome::NoOp, vec![]));
    }
    let hooks_dir = match adapter.hooks_dir.as_ref() {
        Some(d) if adapter.hooks_enabled => d,
        _ => return Ok((HookSyncOutcome::NoOp, vec![])),
    };

    let mut notes = Vec::new();
    let mut wrote = false;
    let mut removed = false;

    for (item, m) in enabled {
        let target = hooks_dir.join(format!("{}.json", m.id));
        refuse_unmanaged_target(&target, read_managed_hooks(&target, &m.id))?;
        let hash = hook_sync::projection_source_hash(item, m);
        let hooks = build_hooks_object(item, m, &hash, &mut notes);
        if hooks.is_empty() {
            match read_managed_hooks(&target, &m.id) {
                FileRead::Ok(existing) if !existing.is_empty() => {
                    fs::remove_file(&target).map_err(|e| io_err(&target, &e))?;
                    removed = true;
                }
                FileRead::Broken => {
                    return Err(HookSyncError {
                        path: target,
                        code: "hook_target_broken_json".to_string(),
                        message:
                            "Hook config file is not valid Grok hook JSON; refusing to overwrite"
                                .to_string(),
                    });
                }
                FileRead::Missing
                | FileRead::Foreign
                | FileRead::ForeignContent
                | FileRead::Ok(_) => {}
            }
            continue;
        }
        let doc = json!({ "hooks": hooks });
        atomic_write_json(&target, &doc).map_err(|e| io_err(&target, &e))?;
        wrote = true;
    }

    if hooks_dir.is_dir() {
        let enabled_ids: std::collections::HashSet<&str> =
            enabled.iter().map(|(_, m)| m.id.as_str()).collect();
        for entry in fs::read_dir(hooks_dir).map_err(|e| io_err(hooks_dir, &e))? {
            let entry = entry.map_err(|e| io_err(hooks_dir, &e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if enabled_ids.contains(stem) {
                continue;
            }
            match read_managed_hooks(&path, stem) {
                FileRead::Ok(hooks) if !hooks.is_empty() => {
                    fs::remove_file(&path).map_err(|e| io_err(&path, &e))?;
                    removed = true;
                }
                FileRead::Missing => {}
                FileRead::Foreign
                | FileRead::ForeignContent
                | FileRead::Broken
                | FileRead::Ok(_) => {}
            }
        }
    }

    let outcome = if wrote {
        HookSyncOutcome::Wrote
    } else if removed {
        HookSyncOutcome::Removed
    } else {
        HookSyncOutcome::NoOp
    };
    Ok((outcome, notes))
}

/// Write or remove one Grok hook file without touching sibling managed hooks.
pub fn sync_single_grok_hook(
    adapter: &ResolvedAdapter,
    item: &CapabilityItem,
    manifest: &HookManifest,
    enabled: bool,
) -> Result<(HookSyncOutcome, Vec<String>), HookSyncError> {
    if adapter.tool_id != ToolId::Grok {
        return Ok((HookSyncOutcome::NoOp, vec![]));
    }
    let Some(dir) = adapter.hooks_dir.as_ref().filter(|_| adapter.hooks_enabled) else {
        return Ok((HookSyncOutcome::NoOp, vec![]));
    };
    let target = dir.join(format!("{}.json", manifest.id));
    if !enabled {
        return remove_managed_grok_file(&target, &manifest.id);
    }
    refuse_unmanaged_target(&target, read_managed_hooks(&target, &manifest.id))?;
    let hash = hook_sync::projection_source_hash(item, manifest);
    let mut notes = Vec::new();
    let hooks = build_hooks_object(item, manifest, &hash, &mut notes);
    if hooks.is_empty() {
        let (outcome, _) = remove_managed_grok_file(&target, &manifest.id)?;
        return Ok((outcome, notes));
    }
    atomic_write_json(&target, &json!({ "hooks": hooks })).map_err(|e| io_err(&target, &e))?;
    Ok((HookSyncOutcome::Wrote, notes))
}

fn remove_managed_grok_file(
    target: &Path,
    hook_id: &str,
) -> Result<(HookSyncOutcome, Vec<String>), HookSyncError> {
    match read_managed_hooks(target, hook_id) {
        FileRead::Ok(hooks) if !hooks.is_empty() => {
            fs::remove_file(target).map_err(|e| io_err(target, &e))?;
            Ok((HookSyncOutcome::Removed, vec![]))
        }
        FileRead::Missing | FileRead::Ok(_) => Ok((HookSyncOutcome::NoOp, vec![])),
        FileRead::Foreign | FileRead::ForeignContent => Ok((HookSyncOutcome::NoOp, vec![])),
        FileRead::Broken => Err(HookSyncError {
            path: target.to_path_buf(),
            code: "hook_target_broken_json".to_string(),
            message: "Hook config file is not valid Grok hook JSON; refusing to overwrite"
                .to_string(),
        }),
    }
}

#[cfg(test)]
#[path = "grok_hook_sync_tests.rs"]
mod tests;
