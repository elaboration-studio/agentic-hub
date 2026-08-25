use super::*;
use crate::adapter_registry::resolve;
use crate::model::SourceRef;
use crate::settings::Settings;

fn manifest(id: &str, events: Vec<HookEventSpec>) -> HookManifest {
    HookManifest {
        id: id.to_string(),
        name: None,
        description: None,
        events,
        command: "${HOOK_DIR}/run.sh".to_string(),
        timeout: Some(30),
        loop_limit: None,
        targets: Some(vec![ToolId::Grok]),
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

fn grok_adapter(dir: &Path) -> ResolvedAdapter {
    let mut s = Settings::default();
    s.tools.grok.enabled = true;
    s.tools.grok.hooks_dir = Some(dir.to_path_buf());
    resolve(&s, ToolId::Grok)
}

#[test]
fn writes_claude_style_pascal_case_groups() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    let hook_dir = dir.path().join("src/fmt");
    fs::create_dir_all(&hook_dir).unwrap();
    fs::write(hook_dir.join("run.sh"), "#!/bin/sh\n").unwrap();

    let item = hook_item("fmt", hook_dir);
    let m = manifest(
        "fmt",
        vec![ev(HookCanonicalEvent::PostToolUse, Some("Edit|Write"))],
    );
    let adapter = grok_adapter(&hooks_dir);

    let (outcome, notes) = sync_grok_hooks(&adapter, &[(&item, &m)]).unwrap();
    assert_eq!(outcome, HookSyncOutcome::Wrote);
    assert!(notes.is_empty());

    let target = hooks_dir.join("fmt.json");
    let content: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert!(content.get("version").is_none());
    let hooks = content.get("hooks").and_then(Value::as_object).unwrap();
    let post = hooks.get("PostToolUse").and_then(Value::as_array).unwrap();
    assert_eq!(post.len(), 1);
    assert_eq!(
        post[0].get("matcher").and_then(Value::as_str),
        Some("Edit|Write")
    );
    let inner = post[0].get("hooks").and_then(Value::as_array).unwrap();
    assert_eq!(
        inner[0].get("type").and_then(Value::as_str),
        Some("command")
    );
    assert!(inner[0]
        .get("command")
        .and_then(Value::as_str)
        .unwrap()
        .ends_with("run.sh"));
    assert_eq!(
        post[0]
            .get("_agenticHub")
            .and_then(|m| m.get("hookId"))
            .and_then(Value::as_str),
        Some("fmt")
    );
}

#[test]
fn tracer_shaped_multi_event_file() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    let hook_dir = dir.path().join("src/tracer");
    fs::create_dir_all(&hook_dir).unwrap();

    let item = hook_item("agentic-hub-usage-tracer-grok", hook_dir);
    let m = manifest(
        "agentic-hub-usage-tracer-grok",
        vec![
            ev(HookCanonicalEvent::UserPromptSubmit, Some(".*")),
            ev(HookCanonicalEvent::PostToolUse, Some(".*")),
            ev(HookCanonicalEvent::PostToolUseFailure, Some(".*")),
        ],
    );
    let adapter = grok_adapter(&hooks_dir);
    sync_grok_hooks(&adapter, &[(&item, &m)]).unwrap();

    let content: Value = serde_json::from_str(
        &fs::read_to_string(hooks_dir.join("agentic-hub-usage-tracer-grok.json")).unwrap(),
    )
    .unwrap();
    let hooks = content.get("hooks").and_then(Value::as_object).unwrap();
    assert!(hooks.contains_key("UserPromptSubmit"));
    assert!(hooks.contains_key("PostToolUse"));
    assert!(hooks.contains_key("PostToolUseFailure"));
}

#[test]
fn unsupported_event_produces_note() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    let item = hook_item("n", dir.path().join("src/n"));
    let m = manifest("n", vec![ev(HookCanonicalEvent::UserPromptExpansion, None)]);
    let adapter = grok_adapter(&hooks_dir);

    let (outcome, notes) = sync_grok_hooks(&adapter, &[(&item, &m)]).unwrap();
    assert_eq!(outcome, HookSyncOutcome::NoOp);
    assert!(notes.iter().any(|n| n.contains("UserPromptExpansion")));
}

#[test]
fn inspect_detects_stale_hash() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    let hook_dir = dir.path().join("src/fmt");
    fs::create_dir_all(&hook_dir).unwrap();
    fs::write(
        hook_dir.join("hook.json"),
        r#"{"id":"fmt","events":[{"name":"Stop"}],"command":"echo"}"#,
    )
    .unwrap();

    let item = hook_item("fmt", hook_dir.clone());
    let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
    let adapter = grok_adapter(&hooks_dir);
    sync_grok_hooks(&adapter, &[(&item, &m)]).unwrap();

    fs::write(
        hook_dir.join("hook.json"),
        r#"{"id":"fmt","events":[{"name":"Stop"}],"command":"echo changed"}"#,
    )
    .unwrap();
    let manifests = HashMap::from([(item.id.clone(), m)]);
    let states = inspect_grok_hooks(&[item], &manifests, &adapter);
    assert_eq!(states[0].state, LinkState::Stale);
}

#[test]
fn sync_refuses_to_overwrite_foreign_hook_file() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let target = hooks_dir.join("fmt.json");
    let foreign = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo user"}]}]}}"#;
    fs::write(&target, foreign).unwrap();

    let item = hook_item("fmt", dir.path().join("src/fmt"));
    let m = manifest("fmt", vec![ev(HookCanonicalEvent::Stop, None)]);
    let adapter = grok_adapter(&hooks_dir);
    let err = sync_grok_hooks(&adapter, &[(&item, &m)]).unwrap_err();
    assert_eq!(err.code, "conflict_real_file_at_target");
    assert_eq!(fs::read_to_string(target).unwrap(), foreign);
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
        r#"{"hooks":{"Stop":[{"_agenticHub":{"hookId":"managed","sourceHash":"x","version":1}}]}}"#,
    )
    .unwrap();
    fs::write(
        &foreign,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo user"}]}]}}"#,
    )
    .unwrap();

    let adapter = grok_adapter(&hooks_dir);
    let (outcome, _) = sync_grok_hooks(&adapter, &[]).unwrap();
    assert_eq!(outcome, HookSyncOutcome::Removed);
    assert!(!managed.exists());
    assert!(foreign.exists());
}

#[test]
fn grok_is_in_default_hook_targets() {
    let m = HookManifest {
        id: "fmt".to_string(),
        name: None,
        description: None,
        events: vec![ev(HookCanonicalEvent::Stop, None)],
        command: "echo".to_string(),
        timeout: None,
        loop_limit: None,
        targets: None,
    };
    assert!(m.effective_targets().contains(&ToolId::Grok));
}

#[test]
fn sync_single_removes_only_that_file() {
    let dir = tempfile::tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    let hook_dir = dir.path().join("src/tracer");
    fs::create_dir_all(&hook_dir).unwrap();
    let item = hook_item("agentic-hub-usage-tracer-grok", hook_dir);
    let m = manifest(
        "agentic-hub-usage-tracer-grok",
        vec![ev(HookCanonicalEvent::PostToolUse, Some(".*"))],
    );
    let adapter = grok_adapter(&hooks_dir);
    sync_single_grok_hook(&adapter, &item, &m, true).unwrap();
    let target = hooks_dir.join("agentic-hub-usage-tracer-grok.json");
    assert!(target.exists());
    sync_single_grok_hook(&adapter, &item, &m, false).unwrap();
    assert!(!target.exists());
}
