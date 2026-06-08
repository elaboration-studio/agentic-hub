//! Hook `json_section` projection — the fourth projection mode. Reads a tool's
//! hook config (Cursor `hooks.json`, Claude `settings.json`, Codex `hooks.json`),
//! partitions managed entries (carrying an `_agenticHub` marker) from foreign
//! ones, and rewrites the file so foreign content is preserved exactly.
//!
//! 1:1 port of the VS Code extension's `HookProjectionSyncService`. See
//! `docs/tech/modules/hook-projection-sync.md`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::adapter_registry::ResolvedAdapter;
use crate::managed_copy::hash_bytes;
use crate::model::{
    CapabilityItem, CapabilityKind, HookSyncError, HookSyncOutcome, LinkState, ToolCapabilityState,
    ToolId,
};

/// Canonical PascalCase event set (Claude/Codex naming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookCanonicalEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    UserPromptSubmit,
    Stop,
    SessionStart,
    SessionEnd,
    PreCompact,
    PostCompact,
    Notification,
    PermissionRequest,
}

impl HookCanonicalEvent {
    fn pascal(self) -> &'static str {
        use HookCanonicalEvent::*;
        match self {
            PreToolUse => "PreToolUse",
            PostToolUse => "PostToolUse",
            PostToolUseFailure => "PostToolUseFailure",
            UserPromptSubmit => "UserPromptSubmit",
            Stop => "Stop",
            SessionStart => "SessionStart",
            SessionEnd => "SessionEnd",
            PreCompact => "PreCompact",
            PostCompact => "PostCompact",
            Notification => "Notification",
            PermissionRequest => "PermissionRequest",
        }
    }

    /// Cursor camelCase key, or `None` if Cursor does not support the event.
    fn cursor_key(self) -> Option<&'static str> {
        use HookCanonicalEvent::*;
        Some(match self {
            PreToolUse => "preToolUse",
            PostToolUse => "postToolUse",
            PostToolUseFailure => "postToolUseFailure",
            UserPromptSubmit => "beforeSubmitPrompt",
            Stop => "stop",
            SessionStart => "sessionStart",
            SessionEnd => "sessionEnd",
            PreCompact => "preCompact",
            PostCompact | Notification | PermissionRequest => return None,
        })
    }

    fn codex_supports(self) -> bool {
        use HookCanonicalEvent::*;
        matches!(
            self,
            PreToolUse
                | PostToolUse
                | UserPromptSubmit
                | Stop
                | SessionStart
                | PreCompact
                | PostCompact
                | PermissionRequest
        )
    }

    fn claude_supports(self) -> bool {
        // Claude is the canonical source and accepts every documented event,
        // including Notification (matcher-free top-level event).
        let _ = self;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEventSpec {
    pub name: HookCanonicalEvent,
    #[serde(default)]
    pub matcher: Option<String>,
}

/// Source-of-truth `hook.json` schema. Unknown fields (e.g. `$schema`) are
/// ignored, so they do not participate in the source hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookManifest {
    /// Defaults to the hook folder name when omitted (see [`load_manifest`]).
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub events: Vec<HookEventSpec>,
    pub command: String,
    #[serde(default)]
    pub timeout: Option<u32>,
    #[serde(default, rename = "loopLimit")]
    pub loop_limit: Option<u32>,
    #[serde(default)]
    pub targets: Option<Vec<ToolId>>,
}

impl HookManifest {
    /// Effective targets: explicit `targets` or the default trio.
    pub fn effective_targets(&self) -> Vec<ToolId> {
        self.targets
            .clone()
            .unwrap_or_else(|| vec![ToolId::Cursor, ToolId::Claude, ToolId::Codex])
    }
}

/// Load and validate a hook manifest from `<hook_dir>/hook.json`.
pub fn load_manifest(hook_dir: &Path) -> Result<HookManifest, String> {
    let path = hook_dir.join("hook.json");
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut manifest: HookManifest =
        serde_json::from_str(&content).map_err(|e| format!("invalid hook.json: {e}"))?;
    if manifest.id.is_empty() {
        manifest.id = hook_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    validate(&manifest)?;
    Ok(manifest)
}

/// Validate manifest invariants. Returns a human-readable error on the first
/// violation. (`targets` with an unknown tool fails earlier during parse.)
pub fn validate(m: &HookManifest) -> Result<(), String> {
    if m.id.is_empty() || !is_kebab_case(&m.id) {
        return Err(format!("hook id '{}' must be kebab-case", m.id));
    }
    if m.events.is_empty() {
        return Err("hook must declare at least one event".to_string());
    }
    if m.command.trim().is_empty() {
        return Err("hook command is required".to_string());
    }
    if matches!(m.loop_limit, Some(0)) {
        return Err("loopLimit must be a positive integer".to_string());
    }
    Ok(())
}

fn is_kebab_case(s: &str) -> bool {
    !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `sha256` of the raw `hook.json` file bytes. Matches the VS Code extension's
/// `hookSourceHash` exactly so managed entries written by either app agree on
/// staleness.
pub fn source_hash(hook_dir: &Path) -> String {
    fs::read(hook_dir.join("hook.json"))
        .map(|b| hash_bytes(&b))
        .unwrap_or_default()
}

/// Replace every `${HOOK_DIR}` with the hook's absolute source folder.
pub fn expand_hook_dir(command: &str, source_path: &Path) -> String {
    command.replace("${HOOK_DIR}", &source_path.to_string_lossy())
}

fn marker(id: &str, hash: &str) -> Value {
    json!({ "hookId": id, "sourceHash": hash, "version": 1 })
}

// ---- Inspection -----------------------------------------------------------

/// Per-(hook, tool) state for every hook item targeting this tool.
pub fn inspect_hooks(
    items: &[CapabilityItem],
    manifests: &HashMap<String, HookManifest>,
    adapter: &ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    let mut out = Vec::new();
    if !adapter.hooks_enabled {
        return out;
    }
    let Some(target) = adapter.hooks_file.clone() else {
        return out;
    };
    let parsed = read_root(&target);

    for item in items.iter().filter(|i| i.kind == CapabilityKind::Hook) {
        let Some(m) = manifests.get(&item.id) else {
            continue;
        };
        if !m.effective_targets().contains(&adapter.tool_id) {
            continue;
        }
        let state = match &parsed {
            RootRead::Missing => LinkState::Disabled,
            RootRead::Foreign => LinkState::ForeignFile,
            RootRead::Broken => LinkState::Broken,
            RootRead::Ok(root) => match find_managed_hash(root, &m.id) {
                Some(hash) if hash == source_hash(&item.source_path) => LinkState::Enabled,
                Some(_) => LinkState::Stale,
                None => LinkState::Disabled,
            },
        };
        out.push(ToolCapabilityState {
            tool: adapter.tool_id,
            item_id: item.id.clone(),
            target_path: target.clone(),
            state,
            current_link_target: Some(target.clone()),
        });
    }
    out
}

enum RootRead {
    Missing,
    Foreign,
    Broken,
    Ok(Map<String, Value>),
}

fn read_root(target: &Path) -> RootRead {
    match fs::symlink_metadata(target) {
        Err(_) => RootRead::Missing,
        Ok(meta) if !meta.is_file() => RootRead::Foreign,
        Ok(_) => match fs::read_to_string(target) {
            Err(_) => RootRead::Broken,
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(Value::Object(map)) => RootRead::Ok(map),
                Ok(_) => RootRead::Broken,
                Err(_) => RootRead::Broken,
            },
        },
    }
}

fn find_managed_hash(root: &Map<String, Value>, hook_id: &str) -> Option<String> {
    let hooks = root.get("hooks")?.as_object()?;
    for arr in hooks.values() {
        for entry in arr.as_array().into_iter().flatten() {
            let agentic = entry.get("_agenticHub");
            if let Some(a) = agentic {
                if a.get("hookId").and_then(Value::as_str) == Some(hook_id) {
                    return a
                        .get("sourceHash")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
        }
    }
    None
}

// ---- Write ----------------------------------------------------------------

/// Read-merge-write the tool's hook file from the enabled hooks. `enabled` pairs
/// are `(item, manifest)` already filtered to those targeting this tool.
pub fn sync_json_hooks(
    adapter: &ResolvedAdapter,
    enabled: &[(&CapabilityItem, &HookManifest)],
) -> Result<(HookSyncOutcome, Vec<String>), HookSyncError> {
    let target = match &adapter.hooks_file {
        Some(t) if adapter.hooks_enabled => t.clone(),
        _ => return Ok((HookSyncOutcome::NoOp, vec![])),
    };
    let tool = adapter.tool_id;
    let cursor_shape = tool == ToolId::Cursor;

    let existed = target.exists();
    let mut root: Map<String, Value> = match read_root(&target) {
        RootRead::Missing => Map::new(),
        RootRead::Ok(map) => map,
        RootRead::Foreign => {
            return Err(HookSyncError {
                path: target,
                code: "conflict_real_file_at_target".to_string(),
                message: "Hook config path is not a regular file".to_string(),
            })
        }
        RootRead::Broken => {
            return Err(HookSyncError {
                path: target,
                code: "hook_target_broken_json".to_string(),
                message: "Hook config file is not valid JSON; refusing to overwrite".to_string(),
            })
        }
    };

    let mut notes = Vec::new();
    let desired = build_desired(tool, cursor_shape, enabled, &mut notes);

    // Partition: keep only foreign (unmarked) entries from the existing file.
    let mut next: Map<String, Value> = Map::new();
    if let Some(Value::Object(existing)) = root.get("hooks") {
        for (event, arr) in existing {
            if let Some(items) = arr.as_array() {
                let theirs: Vec<Value> = items
                    .iter()
                    .filter(|e| e.get("_agenticHub").is_none())
                    .cloned()
                    .collect();
                if !theirs.is_empty() {
                    next.insert(event.clone(), Value::Array(theirs));
                }
            }
        }
    }
    for (event_key, value) in desired {
        next.entry(event_key)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("event value is an array")
            .push(value);
    }

    let hooks_empty = next.is_empty();
    if hooks_empty {
        root.remove("hooks");
    } else {
        if cursor_shape {
            root.entry("version".to_string()).or_insert(json!(1));
        }
        root.insert("hooks".to_string(), Value::Object(next));
    }

    // Removal: only when this is a hooks-only file with nothing foreign left.
    if hooks_empty {
        let hooks_only_file = cursor_shape || matches!(tool, ToolId::Codex | ToolId::Openstandard);
        let no_foreign = root.keys().all(|k| k == "version");
        if existed && hooks_only_file && no_foreign {
            fs::remove_file(&target).map_err(|e| io_err(&target, &e))?;
            return Ok((HookSyncOutcome::Removed, notes));
        }
        if !existed && root.is_empty() {
            return Ok((HookSyncOutcome::NoOp, notes));
        }
    }

    atomic_write_json(&target, &Value::Object(root)).map_err(|e| io_err(&target, &e))?;
    Ok((HookSyncOutcome::Wrote, notes))
}

fn build_desired(
    tool: ToolId,
    cursor_shape: bool,
    enabled: &[(&CapabilityItem, &HookManifest)],
    notes: &mut Vec<String>,
) -> Vec<(String, Value)> {
    let mut desired = Vec::new();
    for (item, m) in enabled {
        let hash = source_hash(&item.source_path);
        for spec in &m.events {
            if cursor_shape {
                match spec.name.cursor_key() {
                    Some(key) => {
                        desired.push((key.to_string(), cursor_entry(item, m, &hash, spec)))
                    }
                    None => notes.push(format!(
                        "{} is not supported by Cursor; entry skipped.",
                        spec.name.pascal()
                    )),
                }
            } else {
                let supported = match tool {
                    ToolId::Codex => spec.name.codex_supports(),
                    _ => spec.name.claude_supports(),
                };
                if supported {
                    desired.push((
                        spec.name.pascal().to_string(),
                        group_entry(item, m, &hash, spec),
                    ));
                } else {
                    notes.push(format!(
                        "{} is not supported by {:?}; entry skipped.",
                        spec.name.pascal(),
                        tool
                    ));
                }
            }
        }
    }
    desired
}

fn cursor_entry(
    item: &CapabilityItem,
    m: &HookManifest,
    hash: &str,
    spec: &HookEventSpec,
) -> Value {
    let mut o = Map::new();
    o.insert(
        "command".to_string(),
        Value::String(expand_hook_dir(&m.command, &item.source_path)),
    );
    if let Some(matcher) = &spec.matcher {
        o.insert("matcher".to_string(), Value::String(matcher.clone()));
    }
    if let Some(t) = m.timeout {
        o.insert("timeout".to_string(), json!(t));
    }
    if let Some(ll) = m.loop_limit {
        o.insert("loop_limit".to_string(), json!(ll));
    }
    o.insert("_agenticHub".to_string(), marker(&m.id, hash));
    Value::Object(o)
}

fn group_entry(item: &CapabilityItem, m: &HookManifest, hash: &str, spec: &HookEventSpec) -> Value {
    let mut inner = Map::new();
    inner.insert("type".to_string(), json!("command"));
    inner.insert(
        "command".to_string(),
        Value::String(expand_hook_dir(&m.command, &item.source_path)),
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

fn atomic_write_json(target: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!("{}\n", serde_json::to_string_pretty(value)?);
    let tmp = target.with_extension("agentic-hooks.tmp");
    if fs::write(&tmp, &body).is_ok() && fs::rename(&tmp, target).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&tmp);
    fs::write(target, &body)
}

fn io_err(path: &Path, e: &std::io::Error) -> HookSyncError {
    HookSyncError {
        path: path.to_path_buf(),
        code: "internal".to_string(),
        message: e.to_string(),
    }
}

/// Load manifests for all hook items, keyed by item id. Items whose `hook.json`
/// fails to parse/validate are omitted (and should be marked `invalid` upstream).
pub fn load_manifests(items: &[CapabilityItem]) -> HashMap<String, HookManifest> {
    let mut map = HashMap::new();
    for item in items.iter().filter(|i| i.kind == CapabilityKind::Hook) {
        if let Ok(m) = load_manifest(&item.source_path) {
            map.insert(item.id.clone(), m);
        }
    }
    map
}

/// Annotate hook items with manifest validation results in place.
pub fn annotate_validation(items: &mut [CapabilityItem]) {
    for item in items.iter_mut().filter(|i| i.kind == CapabilityKind::Hook) {
        if let Err(e) = load_manifest(&item.source_path) {
            item.valid = false;
            item.validation_errors = vec![e];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_registry;
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
            targets: None,
        }
    }

    fn ev(name: HookCanonicalEvent, matcher: Option<&str>) -> HookEventSpec {
        HookEventSpec {
            name,
            matcher: matcher.map(str::to_string),
        }
    }

    fn hook_item(id: &str, source_path: PathBuf) -> CapabilityItem {
        CapabilityItem {
            id: format!("hook:{id}"),
            kind: CapabilityKind::Hook,
            name: id.to_string(),
            source_path,
            relative_path: PathBuf::from(id),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            source: crate::model::SourceRef {
                rel_home: "~/.agentic".into(),
                folder: ".agentic".into(),
            },
            valid: true,
            validation_errors: vec![],
        }
    }

    #[test]
    fn validation_rejects_bad_manifests() {
        assert!(validate(&manifest(
            "Bad_Id",
            vec![ev(HookCanonicalEvent::Stop, None)]
        ))
        .is_err());
        assert!(validate(&manifest("ok", vec![])).is_err());
        let mut m = manifest("ok", vec![ev(HookCanonicalEvent::Stop, None)]);
        m.command = "  ".into();
        assert!(validate(&m).is_err());
        m.command = "run".into();
        m.loop_limit = Some(0);
        assert!(validate(&m).is_err());
    }

    #[test]
    fn source_hash_is_sha256_of_raw_file() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("hooks/fmt");
        fs::create_dir_all(&hook).unwrap();
        let raw =
            r#"{ "$schema": "x", "id": "fmt", "command": "run", "events": [{"name":"Stop"}] }"#;
        fs::write(hook.join("hook.json"), raw).unwrap();
        assert_eq!(source_hash(&hook), hash_bytes(raw.as_bytes()));
    }

    #[test]
    fn load_manifest_falls_back_to_folder_id() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("hooks/auto-format");
        fs::create_dir_all(&hook).unwrap();
        fs::write(
            hook.join("hook.json"),
            r#"{ "command": "run", "events": [{"name":"Stop"}] }"#,
        )
        .unwrap();
        let m = load_manifest(&hook).unwrap();
        assert_eq!(m.id, "auto-format");
    }

    #[test]
    fn expand_hook_dir_replaces_token() {
        let out = expand_hook_dir("${HOOK_DIR}/run.sh --flag", Path::new("/src/hooks/fmt"));
        assert_eq!(out, "/src/hooks/fmt/run.sh --flag");
    }

    #[test]
    fn cursor_sync_preserves_foreign_and_inspects_state() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        // Pre-existing foreign entry + foreign top-level key.
        fs::write(
            &target,
            r#"{ "$schema": "x", "hooks": { "postToolUse": [ { "command": "user.sh" } ] } }"#,
        )
        .unwrap();

        // A real hook source folder so the file-based source hash is meaningful.
        let hook_src = dir.path().join("hooks/fmt");
        fs::create_dir_all(&hook_src).unwrap();
        fs::write(
            hook_src.join("hook.json"),
            r#"{ "id": "fmt", "command": "run.sh", "events": [{"name":"PostToolUse","matcher":"Edit|Write"}] }"#,
        )
        .unwrap();

        let adapter = {
            let mut s = Settings::default();
            s.tools.cursor.hooks_file = Some(target.clone());
            adapter_registry::resolve(&s, ToolId::Cursor)
        };
        let m = manifest(
            "fmt",
            vec![ev(HookCanonicalEvent::PostToolUse, Some("Edit|Write"))],
        );
        let item = hook_item("fmt", hook_src.clone());

        let (outcome, notes) = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Wrote);
        assert!(notes.is_empty());

        let written: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        let arr = written["hooks"]["postToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "foreign + managed coexist");
        assert_eq!(written["$schema"], json!("x"), "foreign key preserved");
        // Marker carries the bare manifest id, matching the VS Code extension.
        let managed = arr.iter().find(|e| e.get("_agenticHub").is_some()).unwrap();
        assert_eq!(managed["_agenticHub"]["hookId"], json!("fmt"));

        // Inspect: enabled (file hash matches the written marker).
        let mut manifests = HashMap::new();
        manifests.insert(item.id.clone(), m);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Enabled);

        // Stale: edit the source file so its hash no longer matches the marker.
        fs::write(
            hook_src.join("hook.json"),
            r#"{ "id": "fmt", "command": "other.sh", "events": [{"name":"PostToolUse","matcher":"Edit|Write"}] }"#,
        )
        .unwrap();
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Stale);
    }

    #[test]
    fn claude_two_level_group_shape() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        fs::write(&target, r#"{ "model": "opus", "hooks": {} }"#).unwrap();
        let adapter = {
            let mut s = Settings::default();
            s.tools.claude.hooks_file = Some(target.clone());
            adapter_registry::resolve(&s, ToolId::Claude)
        };
        let m = manifest(
            "fmt",
            vec![ev(HookCanonicalEvent::PostToolUse, Some("Edit"))],
        );
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));

        sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        let written: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        let group = &written["hooks"]["PostToolUse"][0];
        assert_eq!(group["matcher"], json!("Edit"));
        assert_eq!(group["hooks"][0]["type"], json!("command"));
        assert_eq!(group["hooks"][0]["timeout"], json!(30));
        assert_eq!(group["_agenticHub"]["hookId"], json!("fmt"));
        assert_eq!(written["model"], json!("opus"), "foreign key preserved");
    }

    #[test]
    fn empty_managed_set_removes_hooks_only_file_but_keeps_foreign() {
        let dir = tempfile::tempdir().unwrap();

        // Cursor hooks-only file with only a managed entry -> deleted when emptied.
        let target = dir.path().join("hooks.json");
        let adapter = {
            let mut s = Settings::default();
            s.tools.cursor.hooks_file = Some(target.clone());
            adapter_registry::resolve(&s, ToolId::Cursor)
        };
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));
        sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert!(target.exists());
        let (outcome, _) = sync_json_hooks(&adapter, &[]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Removed);
        assert!(!target.exists());

        // Claude settings.json keeps the file (foreign settings present).
        let claude_target = dir.path().join("settings.json");
        fs::write(&claude_target, r#"{ "model": "opus" }"#).unwrap();
        let claude = {
            let mut s = Settings::default();
            s.tools.claude.hooks_file = Some(claude_target.clone());
            adapter_registry::resolve(&s, ToolId::Claude)
        };
        let (outcome, _) = sync_json_hooks(&claude, &[]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Wrote);
        assert!(claude_target.exists());
    }

    fn cursor_adapter(target: &Path) -> ResolvedAdapter {
        let mut s = Settings::default();
        s.tools.cursor.hooks_file = Some(target.to_path_buf());
        adapter_registry::resolve(&s, ToolId::Cursor)
    }

    fn codex_adapter(target: &Path) -> ResolvedAdapter {
        let mut s = Settings::default();
        s.tools.codex.hooks_file = Some(target.to_path_buf());
        adapter_registry::resolve(&s, ToolId::Codex)
    }

    #[test]
    fn sync_rejects_directory_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        fs::create_dir_all(&target).unwrap();
        let adapter = cursor_adapter(&target);
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));
        let err = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap_err();
        assert_eq!(err.code, "conflict_real_file_at_target");
    }

    #[test]
    fn sync_refuses_to_overwrite_broken_json() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        fs::write(&target, "{ not valid json").unwrap();
        let adapter = cursor_adapter(&target);
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));
        let err = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap_err();
        assert_eq!(err.code, "hook_target_broken_json");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "{ not valid json",
            "broken file left untouched"
        );
    }

    #[test]
    fn codex_uses_group_shape_and_notes_unsupported_event() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        let adapter = codex_adapter(&target);
        // PostToolUse is supported; Notification is not on Codex.
        let m = manifest(
            "fmt",
            vec![
                ev(HookCanonicalEvent::PostToolUse, Some("Edit")),
                ev(HookCanonicalEvent::Notification, None),
            ],
        );
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));

        let (outcome, notes) = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Wrote);
        assert!(
            notes.iter().any(|n| n.contains("Notification")),
            "unsupported event noted: {notes:?}"
        );

        let written: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        let group = &written["hooks"]["PostToolUse"][0];
        assert_eq!(group["matcher"], json!("Edit"));
        assert_eq!(group["hooks"][0]["type"], json!("command"));
        assert_eq!(group["_agenticHub"]["hookId"], json!("fmt"));
        assert!(
            written["hooks"].get("Notification").is_none(),
            "unsupported event not written"
        );
    }

    #[test]
    fn cursor_notes_unsupported_event() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        let adapter = cursor_adapter(&target);
        // PostCompact has no Cursor key.
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::PostCompact, None)]);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));

        let (outcome, notes) = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        // Nothing managed to write and no pre-existing file -> NoOp.
        assert_eq!(outcome, HookSyncOutcome::NoOp);
        assert!(notes.iter().any(|n| n.contains("PostCompact")), "{notes:?}");
    }

    #[test]
    fn inspect_reports_disabled_foreign_and_broken() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));
        let mut manifests = HashMap::new();
        manifests.insert(item.id.clone(), m);

        // Missing file -> Disabled.
        let target = dir.path().join("missing.json");
        let adapter = cursor_adapter(&target);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Disabled);

        // Directory at the target -> ForeignFile.
        let dir_target = dir.path().join("dir.json");
        fs::create_dir_all(&dir_target).unwrap();
        let adapter = cursor_adapter(&dir_target);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::ForeignFile);

        // Non-JSON text -> Broken.
        let broken = dir.path().join("broken.json");
        fs::write(&broken, "{ nope").unwrap();
        let adapter = cursor_adapter(&broken);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Broken);

        // Valid JSON without a managed entry -> Disabled.
        let empty = dir.path().join("empty.json");
        fs::write(&empty, r#"{ "hooks": {} }"#).unwrap();
        let adapter = cursor_adapter(&empty);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Disabled);
    }

    #[test]
    fn targets_filter_excludes_untargeted_tool() {
        // Default targets is the trio (Cursor included).
        let default_m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        assert!(default_m.effective_targets().contains(&ToolId::Cursor));

        // Explicit targets honored: a Claude-only hook excludes Cursor.
        let mut claude_only = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
        claude_only.targets = Some(vec![ToolId::Claude]);
        assert_eq!(claude_only.effective_targets(), vec![ToolId::Claude]);

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hooks.json");
        let adapter = cursor_adapter(&target);
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));
        let mut manifests = HashMap::new();
        manifests.insert(item.id.clone(), claude_only);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert!(states.is_empty(), "untargeted tool yields no state");
    }

    #[test]
    fn load_manifests_keys_by_id_and_drops_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("hooks/good");
        fs::create_dir_all(&good).unwrap();
        fs::write(
            good.join("hook.json"),
            r#"{ "command": "run", "events": [{"name":"Stop"}] }"#,
        )
        .unwrap();
        let bad = dir.path().join("hooks/bad");
        fs::create_dir_all(&bad).unwrap();
        // Missing the required `command` field -> fails to parse.
        fs::write(bad.join("hook.json"), r#"{ "events": [{"name":"Stop"}] }"#).unwrap();

        let items = vec![hook_item("good", good), hook_item("bad", bad)];
        let map = load_manifests(&items);
        assert!(map.contains_key("hook:good"));
        assert!(!map.contains_key("hook:bad"), "invalid manifest dropped");
    }

    #[test]
    fn annotate_validation_flags_bad_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("hooks/bad");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("hook.json"), r#"{ "events": [] }"#).unwrap();
        let mut items = vec![hook_item("bad", bad)];
        annotate_validation(&mut items);
        assert!(!items[0].valid);
        assert!(!items[0].validation_errors.is_empty());
    }
}
