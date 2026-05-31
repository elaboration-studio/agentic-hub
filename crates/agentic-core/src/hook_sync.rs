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
        // Claude is the canonical source; supports everything except Notification.
        self != HookCanonicalEvent::Notification
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
    let manifest: HookManifest =
        serde_json::from_str(&content).map_err(|e| format!("invalid hook.json: {e}"))?;
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

/// `sha256` of the canonical manifest JSON (struct re-serialized → stable key
/// order; `$schema` already dropped by parsing).
pub fn source_hash(m: &HookManifest) -> String {
    let json = serde_json::to_string(m).unwrap_or_default();
    hash_bytes(json.as_bytes())
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
            RootRead::Ok(root) => match find_managed_hash(root, &item.id) {
                Some(hash) if hash == source_hash(m) => LinkState::Enabled,
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
        let hooks_only_file = cursor_shape || tool == ToolId::Codex;
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
        let hash = source_hash(m);
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
    if let Some(ll) = m.loop_limit {
        o.insert("loop_limit".to_string(), json!(ll));
    }
    o.insert("_agenticHub".to_string(), marker(&item.id, hash));
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
    g.insert("_agenticHub".to_string(), marker(&item.id, hash));
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
    fn source_hash_ignores_schema_and_is_stable() {
        let with_schema = r#"{ "$schema": "agentic-hub.hook.v1", "id": "h", "events": [{"name":"Stop"}], "command": "x" }"#;
        let without = r#"{ "id": "h", "command": "x", "events": [{"name":"Stop"}] }"#;
        let a: HookManifest = serde_json::from_str(with_schema).unwrap();
        let b: HookManifest = serde_json::from_str(without).unwrap();
        assert_eq!(source_hash(&a), source_hash(&b));
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

        let adapter = {
            let mut s = Settings::default();
            s.tools.cursor.hooks_file = Some(target.clone());
            adapter_registry::resolve(&s, ToolId::Cursor)
        };
        let m = manifest(
            "fmt",
            vec![ev(HookCanonicalEvent::PostToolUse, Some("Edit|Write"))],
        );
        let item = hook_item("fmt", dir.path().join("hooks/fmt"));

        let (outcome, notes) = sync_json_hooks(&adapter, &[(&item, &m)]).unwrap();
        assert_eq!(outcome, HookSyncOutcome::Wrote);
        assert!(notes.is_empty());

        let written: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        let arr = written["hooks"]["postToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "foreign + managed coexist");
        assert_eq!(written["$schema"], json!("x"), "foreign key preserved");

        // Inspect: enabled (hash matches).
        let mut manifests = HashMap::new();
        manifests.insert(item.id.clone(), m.clone());
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests, &adapter);
        assert_eq!(states[0].state, LinkState::Enabled);

        // Stale: mutate the manifest so the hash no longer matches.
        let mut m2 = m.clone();
        m2.command = "${HOOK_DIR}/other.sh".into();
        let mut manifests2 = HashMap::new();
        manifests2.insert(item.id.clone(), m2);
        let states = inspect_hooks(std::slice::from_ref(&item), &manifests2, &adapter);
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
        assert_eq!(group["_agenticHub"]["hookId"], json!("hook:fmt"));
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
}
