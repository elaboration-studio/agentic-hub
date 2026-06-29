//! Kiro hook projection — one v1 JSON file per hook id under `~/.kiro/hooks/`.
//! See `docs/tech/modules/kiro-tool-adapter.md`.

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

fn kiro_trigger(event: HookCanonicalEvent) -> Option<&'static str> {
    use HookCanonicalEvent::{
        Notification, PermissionRequest, PostCompact, PostToolUse, PostToolUseFailure, PreCompact,
        PreToolUse, SessionEnd, SessionStart, Stop, UserPromptSubmit,
    };
    match event {
        PreToolUse => Some("PreToolUse"),
        PostToolUse => Some("PostToolUse"),
        UserPromptSubmit => Some("UserPromptSubmit"),
        Stop => Some("Stop"),
        SessionStart => Some("SessionStart"),
        PostToolUseFailure | SessionEnd | PreCompact | PostCompact | Notification
        | PermissionRequest => None,
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

fn build_entry(item: &CapabilityItem, m: &HookManifest, hash: &str, spec: &HookEventSpec) -> Value {
    let mut entry = Map::new();
    entry.insert(
        "name".to_string(),
        Value::String(m.name.clone().unwrap_or_else(|| m.id.clone())),
    );
    entry.insert(
        "trigger".to_string(),
        Value::String(
            kiro_trigger(spec.name)
                .expect("filtered upstream")
                .to_string(),
        ),
    );
    if let Some(matcher) = &spec.matcher {
        entry.insert("matcher".to_string(), Value::String(matcher.clone()));
    }
    entry.insert(
        "action".to_string(),
        json!({
            "type": "command",
            "command": hook_sync::expand_hook_dir(&m.command, &item.source_path),
        }),
    );
    if let Some(t) = m.timeout {
        entry.insert("timeout".to_string(), json!(t));
    }
    entry.insert("enabled".to_string(), json!(true));
    entry.insert("_agenticHub".to_string(), marker(&m.id, hash));
    Value::Object(entry)
}

fn build_hooks_array(
    item: &CapabilityItem,
    m: &HookManifest,
    hash: &str,
    notes: &mut Vec<String>,
) -> Vec<Value> {
    let mut out = Vec::new();
    for spec in &m.events {
        match kiro_trigger(spec.name) {
            Some(_) => out.push(build_entry(item, m, hash, spec)),
            None => notes.push(format!(
                "{} is not supported by Kiro; entry skipped.",
                spec.name.pascal()
            )),
        }
    }
    out
}

enum FileRead {
    Missing,
    Foreign,
    Broken,
    ForeignContent,
    Ok(Vec<Value>),
}

fn read_managed_entries(path: &Path, hook_id: &str) -> FileRead {
    match fs::symlink_metadata(path) {
        Err(_) => FileRead::Missing,
        Ok(meta) if !meta.is_file() => FileRead::Foreign,
        Ok(_) => match fs::read_to_string(path) {
            Err(_) => FileRead::Broken,
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(Value::Object(root)) => {
                    let Some(arr) = root.get("hooks").and_then(Value::as_array) else {
                        return FileRead::Broken;
                    };
                    if arr.is_empty() {
                        return FileRead::Ok(Vec::new());
                    }
                    let all_managed = arr.iter().all(|e| {
                        e.get("_agenticHub")
                            .and_then(|a| a.get("hookId"))
                            .and_then(Value::as_str)
                            == Some(hook_id)
                    });
                    if !all_managed {
                        return FileRead::ForeignContent;
                    }
                    FileRead::Ok(arr.clone())
                }
                Ok(_) => FileRead::Broken,
                Err(_) => FileRead::Broken,
            },
        },
    }
}

fn managed_hash(entries: &[Value], hook_id: &str) -> Option<String> {
    entries.iter().find_map(|e| {
        let agentic = e.get("_agenticHub")?;
        if agentic.get("hookId").and_then(Value::as_str) == Some(hook_id) {
            agentic
                .get("sourceHash")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        }
    })
}

/// Per-(hook, tool) state for Kiro hook files.
pub fn inspect_kiro_hooks(
    items: &[CapabilityItem],
    manifests: &HashMap<String, HookManifest>,
    adapter: &ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    let mut out = Vec::new();
    if adapter.tool_id != ToolId::Kiro || !adapter.hooks_enabled {
        return out;
    }
    if adapter.hooks_dir.is_none() {
        return out;
    }

    for item in items.iter().filter(|i| i.kind == CapabilityKind::Hook) {
        let Some(m) = manifests.get(&item.id) else {
            continue;
        };
        if !m.effective_targets().contains(&ToolId::Kiro) {
            continue;
        }
        let Some(target) = hook_target(adapter, &m.id) else {
            continue;
        };
        let hash = hook_sync::source_hash(&item.source_path);
        let state = match read_managed_entries(&target, &m.id) {
            FileRead::Missing => LinkState::Disabled,
            FileRead::Foreign | FileRead::ForeignContent => LinkState::ForeignFile,
            FileRead::Broken => LinkState::Broken,
            FileRead::Ok(entries) if entries.is_empty() => LinkState::Disabled,
            FileRead::Ok(entries) => match managed_hash(&entries, &m.id) {
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
    let tmp = target.with_extension("agentic-kiro-hooks.tmp");
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
        FileRead::Ok(entries) if !entries.is_empty() => Ok(()),
        FileRead::Foreign | FileRead::ForeignContent | FileRead::Ok(_) => Err(HookSyncError {
            path: path.to_path_buf(),
            code: "conflict_real_file_at_target".to_string(),
            message: "Hook config file is not fully managed; refusing to overwrite".to_string(),
        }),
        FileRead::Broken => Err(HookSyncError {
            path: path.to_path_buf(),
            code: "hook_target_broken_json".to_string(),
            message: "Hook config file is not valid Kiro hook JSON; refusing to overwrite"
                .to_string(),
        }),
    }
}

/// Write or remove Kiro hook JSON files for enabled hooks targeting Kiro.
pub fn sync_kiro_hooks(
    adapter: &ResolvedAdapter,
    enabled: &[(&CapabilityItem, &HookManifest)],
) -> Result<(HookSyncOutcome, Vec<String>), HookSyncError> {
    if adapter.tool_id != ToolId::Kiro {
        return Ok((HookSyncOutcome::NoOp, vec![]));
    }
    let hooks_dir = match adapter.hooks_dir.as_ref() {
        Some(d) if adapter.hooks_enabled => d,
        _ => return Ok((HookSyncOutcome::NoOp, vec![])),
    };

    let mut notes = Vec::new();
    let mut wrote = false;
    let mut removed = false;

    // Sync each enabled hook to its own file.
    for (item, m) in enabled {
        let target = hooks_dir.join(format!("{}.json", m.id));
        refuse_unmanaged_target(&target, read_managed_entries(&target, &m.id))?;
        let hash = hook_sync::source_hash(&item.source_path);
        let entries = build_hooks_array(item, m, &hash, &mut notes);
        if entries.is_empty() {
            match read_managed_entries(&target, &m.id) {
                FileRead::Ok(existing) if !existing.is_empty() => {
                    fs::remove_file(&target).map_err(|e| io_err(&target, &e))?;
                    removed = true;
                }
                FileRead::Broken => {
                    return Err(HookSyncError {
                        path: target,
                        code: "hook_target_broken_json".to_string(),
                        message:
                            "Hook config file is not valid Kiro hook JSON; refusing to overwrite"
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
        let doc = json!({ "version": "v1", "hooks": entries });
        atomic_write_json(&target, &doc).map_err(|e| io_err(&target, &e))?;
        wrote = true;
    }

    // Remove managed files for hooks no longer enabled (only hub-owned files).
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
            match read_managed_entries(&path, stem) {
                FileRead::Ok(entries) if !entries.is_empty() => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_registry::resolve;
    use crate::model::SourceRef;
    use crate::settings::Settings;
    use std::path::PathBuf;

    fn manifest(id: &str, events: Vec<HookEventSpec>) -> HookManifest {
        HookManifest {
            id: id.to_string(),
            name: None,
            description: None,
            events,
            command: "${HOOK_DIR}/run.sh".to_string(),
            timeout: Some(30),
            loop_limit: None,
            targets: Some(vec![ToolId::Kiro]),
        }
    }

    fn ev(name: HookCanonicalEvent, matcher: Option<&str>) -> HookEventSpec {
        HookEventSpec {
            name,
            matcher: matcher.map(str::to_string),
        }
    }

    fn hook_item(id: &str, dir: PathBuf) -> CapabilityItem {
        CapabilityItem {
            id: format!("hook:{id}"),
            kind: CapabilityKind::Hook,
            name: id.to_string(),
            source_path: dir,
            relative_path: PathBuf::from(id),
            source_id: "test".into(),
            source_label: "Test".into(),
            source: SourceRef {
                rel_home: "~/.agentic".into(),
                folder: ".agentic".into(),
            },
            valid: true,
            validation_errors: vec![],
        }
    }

    fn kiro_adapter(dir: &Path) -> ResolvedAdapter {
        let mut s = Settings::default();
        s.tools.kiro.enabled = true;
        s.tools.kiro.hooks_dir = Some(dir.to_path_buf());
        resolve(&s, ToolId::Kiro)
    }

    #[test]
    fn writes_v1_json_with_mapped_events() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let hook_dir = dir.path().join("src/fmt");
        fs::create_dir_all(&hook_dir).unwrap();
        fs::write(
            hook_dir.join("hook.json"),
            r#"{"id":"fmt","events":[{"name":"PostToolUse","matcher":"Edit"}],"command":"echo ok"}"#,
        )
        .unwrap();
        fs::write(hook_dir.join("run.sh"), "#!/bin/sh\n").unwrap();

        let item = hook_item("fmt", hook_dir);
        let m = manifest(
            "fmt",
            vec![ev(HookCanonicalEvent::PostToolUse, Some("Edit"))],
        );
        let adapter = kiro_adapter(&hooks_dir);

        let (outcome, notes) = sync_kiro_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Wrote);
        assert!(notes.is_empty());

        let target = hooks_dir.join("fmt.json");
        let content: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(content.get("version").and_then(Value::as_str), Some("v1"));
        let hooks = content.get("hooks").and_then(Value::as_array).unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(
            hooks[0].get("trigger").and_then(Value::as_str),
            Some("PostToolUse")
        );
        assert!(hooks[0].get("_agenticHub").is_some());
    }

    #[test]
    fn unsupported_event_produces_note() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let hook_dir = dir.path().join("src/n");
        fs::create_dir_all(&hook_dir).unwrap();
        fs::write(
            hook_dir.join("hook.json"),
            r#"{"id":"n","events":[{"name":"Notification"}],"command":"echo"}"#,
        )
        .unwrap();

        let item = hook_item("n", hook_dir);
        let m = manifest("n", vec![ev(HookCanonicalEvent::Notification, None)]);
        let adapter = kiro_adapter(&hooks_dir);

        let (outcome, notes) = sync_kiro_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::NoOp);
        assert!(notes.iter().any(|n| n.contains("Notification")));
    }

    #[test]
    fn unsupported_reapply_removes_previous_managed_file() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let item = hook_item("fmt", dir.path().join("src/fmt"));
        let supported = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let unsupported = manifest("fmt", vec![ev(HookCanonicalEvent::Notification, None)]);
        let adapter = kiro_adapter(&hooks_dir);

        sync_kiro_hooks(&adapter, &[(&item, &supported)]).unwrap();
        let target = hooks_dir.join("fmt.json");
        assert!(target.exists());

        let (outcome, _) = sync_kiro_hooks(&adapter, &[(&item, &unsupported)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Removed);
        assert!(!target.exists());
    }

    #[test]
    fn inspect_detects_stale_hash() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let hook_dir = dir.path().join("src/fmt");
        fs::create_dir_all(&hook_dir).unwrap();
        fs::write(
            hook_dir.join("hook.json"),
            r#"{"id":"fmt","events":[{"name":"Stop"}],"command":"echo"}"#,
        )
        .unwrap();

        let item = hook_item("fmt", hook_dir.clone());
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let adapter = kiro_adapter(&hooks_dir);

        sync_kiro_hooks(&adapter, &[(&item, &m)]).unwrap();
        fs::write(
            hook_dir.join("hook.json"),
            r#"{"id":"fmt","events":[{"name":"Stop"}],"command":"echo changed"}"#,
        )
        .unwrap();

        let manifests = HashMap::from([(item.id.clone(), m)]);
        let states = inspect_kiro_hooks(&[item], &manifests, &adapter);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].state, LinkState::Stale);
    }

    #[test]
    fn sync_refuses_to_overwrite_foreign_hook_file() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let target = hooks_dir.join("fmt.json");
        let foreign = r#"{"version":"v1","hooks":[{"name":"user-hook","trigger":"Stop"}]}"#;
        fs::write(&target, foreign).unwrap();

        let item = hook_item("fmt", dir.path().join("src/fmt"));
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let adapter = kiro_adapter(&hooks_dir);

        let err = sync_kiro_hooks(&adapter, &[(&item, &m)]).unwrap_err();
        assert_eq!(err.code, "conflict_real_file_at_target");
        assert_eq!(fs::read_to_string(target).unwrap(), foreign);
    }

    #[test]
    fn sync_refuses_to_overwrite_broken_hook_file() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let target = hooks_dir.join("fmt.json");
        fs::write(&target, "{ broken").unwrap();

        let item = hook_item("fmt", dir.path().join("src/fmt"));
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let adapter = kiro_adapter(&hooks_dir);

        let err = sync_kiro_hooks(&adapter, &[(&item, &m)]).unwrap_err();
        assert_eq!(err.code, "hook_target_broken_json");
        assert_eq!(fs::read_to_string(target).unwrap(), "{ broken");
    }

    #[test]
    fn empty_sync_removes_managed_file_but_preserves_foreign_file() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let managed = hooks_dir.join("managed.json");
        let foreign = hooks_dir.join("foreign.json");
        fs::write(
            &managed,
            r#"{"version":"v1","hooks":[{"_agenticHub":{"hookId":"managed","sourceHash":"x","version":1}}]}"#,
        )
        .unwrap();
        fs::write(
            &foreign,
            r#"{"version":"v1","hooks":[{"name":"user-hook","trigger":"Stop"}]}"#,
        )
        .unwrap();

        let adapter = kiro_adapter(&hooks_dir);
        let (outcome, _) = sync_kiro_hooks(&adapter, &[]).unwrap();

        assert_eq!(outcome, HookSyncOutcome::Removed);
        assert!(!managed.exists());
        assert!(foreign.exists());
    }

    #[test]
    fn kiro_not_in_default_hook_targets() {
        let m = HookManifest {
            id: "fmt".to_string(),
            name: None,
            description: None,
            events: vec![HookEventSpec {
                name: HookCanonicalEvent::Stop,
                matcher: None,
            }],
            command: "echo".to_string(),
            timeout: None,
            loop_limit: None,
            targets: None,
        };
        assert!(!m.effective_targets().contains(&ToolId::Kiro));
    }
}
