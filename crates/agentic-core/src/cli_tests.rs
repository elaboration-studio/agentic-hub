use std::cell::Cell;
use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::*;
use crate::agent_version::agent_version;
use crate::scanner::scan;
use crate::settings::SourceConfig;
use crate::suite_store::SuiteStore;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

/// A home with one source root and a suites file holding an agent suite
/// (`cto-1`) and a plain suite (`plain-1`).
fn home() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    write(&src.join("skills/cto/tdd/SKILL.md"), "# tdd");
    write(&src.join("rules/core.md"), "Be precise.");
    write(
        &src.join("mcp/github/mcp.json"),
        r#"{ "name": "github", "transport": "stdio", "command": "github-mcp" }"#,
    );
    write(
        &dir.path().join("suites.json"),
        r#"{ "version": 1, "suites": [
            { "id": "cto-1", "name": "Arno's CTO", "description": "Architecture",
              "capabilities": ["skill:cto/tdd", "rule:core.md", "mcp:github", "skill:gone"],
              "agent": { "emoji": "⚒️", "instructions": "Lead.", "requiredClis": ["gh"] },
              "createdAt": "t", "updatedAt": "t" },
            { "id": "plain-1", "name": "Plain", "description": null,
              "capabilities": ["skill:cto/tdd"], "createdAt": "t", "updatedAt": "t" }
        ] }"#,
    );
    dir
}

fn context(home: &Path) -> CliContext {
    let settings = Settings {
        sources: vec![SourceConfig {
            id: String::new(),
            label: "Arno".into(),
            path: home.join("src"),
        }],
        suites_path: Some(home.join("suites.json")),
        ..Settings::default()
    };
    CliContext {
        settings,
        bundles_root: home.join(".agentic-hub/bundles"),
        probe: Box::new(|id| id == "gh"),
        now: SystemTime::now(),
    }
}

/// Run against `home`; returns (exit code, parsed stdout, loader was called).
fn run_in(home: &Path, words: &[&str]) -> (i32, Value, bool) {
    let called = Cell::new(false);
    let mut out = Vec::new();
    let code = run_with(&args(words), &mut out, || {
        called.set(true);
        Ok(context(home))
    });
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.ends_with('\n') && text.trim_end().lines().count() == 1,
        "{text:?}"
    );
    (code, serde_json::from_str(&text).unwrap(), called.get())
}

fn error_code(v: &Value) -> &str {
    v["error"]["code"].as_str().unwrap()
}

#[test]
fn version_prints_schema_contract_and_app_version_without_loading() {
    let h = home();
    let (code, v, loaded) = run_in(h.path(), &["version"]);
    assert_eq!(code, 0);
    assert_eq!(
        v,
        json!({ "schema": 1, "contract": 1, "appVersion": env!("CARGO_PKG_VERSION") })
    );
    assert!(!loaded);
}

#[test]
fn json_flag_is_accepted_and_ignored_anywhere() {
    let h = home();
    assert_eq!(
        run_in(h.path(), &["--json", "version"]).1,
        run_in(h.path(), &["version"]).1
    );
    assert_eq!(run_in(h.path(), &["agents", "list", "--json"]).0, 0);
}

#[test]
fn agents_list_summarises_every_suite() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["agents", "list"]);
    assert_eq!(code, 0);
    let suites = SuiteStore::with_path(h.path().join("suites.json"))
        .list()
        .unwrap();
    let items = scan(&h.path().join("src")).items;
    assert_eq!(
        v,
        json!({ "schema": 1, "agents": [
            { "id": "cto-1", "name": "Arno's CTO", "emoji": "⚒️", "description": "Architecture",
              "version": agent_version(&suites[0], &items), "isAgent": true,
              "capabilityCounts": { "skill": 2, "agent": 0, "rule": 1, "hook": 0, "command": 0, "mcp": 1 },
              "requiredClis": ["gh"] },
            { "id": "plain-1", "name": "Plain", "emoji": null, "description": null,
              "version": agent_version(&suites[1], &items), "isAgent": false,
              "capabilityCounts": { "skill": 1, "agent": 0, "rule": 0, "hook": 0, "command": 0, "mcp": 0 },
              "requiredClis": [] }
        ] })
    );
}

#[test]
fn agents_show_adds_sorted_capability_ids_to_the_summary() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["agents", "show", "cto-1"]);
    assert_eq!(code, 0);
    assert_eq!(v["schema"], 1);
    assert_eq!(v["agent"]["id"], "cto-1");
    assert_eq!(v["agent"]["isAgent"], true);
    assert_eq!(
        v["agent"]["capabilities"],
        json!(["mcp:github", "rule:core.md", "skill:cto/tdd", "skill:gone"])
    );
}

#[test]
fn agents_show_unknown_id_is_not_found_exit_3() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["agents", "show", "nope"]);
    assert_eq!(code, 3);
    assert_eq!(
        v,
        json!({ "schema": 1, "error": { "code": "NOT_FOUND", "message": "No suite with id nope" } })
    );
}

#[test]
fn bundle_prints_the_manifest_with_contract_field_names() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["bundle", "cto-1", "--harness", "cursor"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["schema"], 1);
    let bundle = v["bundle"].as_object().unwrap();
    let mut keys: Vec<&str> = bundle.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "agentId",
            "harness",
            "instructionsFile",
            "mcpServers",
            "mount",
            "name",
            "pluginDir",
            "requiredClis",
            "root",
            "skills",
            "skipped",
            "version"
        ]
    );
    assert_eq!(bundle["agentId"], "cto-1");
    assert_eq!(bundle["mount"], "plugin");
    assert_eq!(
        bundle["requiredClis"],
        json!([{ "id": "gh", "installed": true }])
    );
    assert_eq!(
        bundle["skipped"],
        json!([{ "capability": "skill:gone", "reason": "not found" }])
    );
    let plugin_dir = bundle["pluginDir"].as_str().unwrap();
    assert!(Path::new(plugin_dir).join("skills/tdd/SKILL.md").is_file());
    assert!(plugin_dir.starts_with(h.path().join(".agentic-hub/bundles").to_str().unwrap()));
}

#[test]
fn bundle_accepts_the_equals_form_of_harness() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["bundle", "cto-1", "--harness=codex", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(v["bundle"]["mount"], "prompt");
    assert_eq!(v["bundle"]["pluginDir"], Value::Null);
}

#[test]
fn bundle_unknown_suite_is_not_found_exit_3() {
    let h = home();
    let (code, v, _) = run_in(h.path(), &["bundle", "nope", "--harness", "claude"]);
    assert_eq!(code, 3);
    assert_eq!(error_code(&v), "NOT_FOUND");
}

#[test]
fn unsupported_harness_is_exit_4_before_any_io() {
    let h = home();
    let (code, v, loaded) = run_in(h.path(), &["bundle", "cto-1", "--harness", "kiro"]);
    assert_eq!(code, 4);
    assert_eq!(error_code(&v), "UNSUPPORTED_HARNESS");
    assert!(!loaded);
}

#[test]
fn bad_id_is_exit_2_before_any_io() {
    let h = home();
    for words in [
        &["bundle", "../etc", "--harness", "cursor"][..],
        &["agents", "show", "a/b"][..],
    ] {
        let (code, v, loaded) = run_in(h.path(), words);
        assert_eq!(code, 2, "{words:?}");
        assert_eq!(error_code(&v), "INVALID_ARGUMENT");
        assert!(!loaded, "{words:?}");
    }
    assert!(!h.path().join(".agentic-hub").exists());
}

#[test]
fn malformed_invocations_are_exit_2() {
    let h = home();
    for words in [
        &[][..],
        &["frobnicate"][..],
        &["agents"][..],
        &["agents", "list", "extra"][..],
        &["agents", "show"][..],
        &["bundle", "cto-1"][..],
        &["bundle", "cto-1", "--harness"][..],
        &["version", "--verbose"][..],
        &["version", "--harness", "cursor"][..],
    ] {
        let (code, v, loaded) = run_in(h.path(), words);
        assert_eq!(code, 2, "{words:?}");
        assert_eq!(error_code(&v), "INVALID_ARGUMENT", "{words:?}");
        assert!(!loaded, "{words:?}");
    }
}

#[test]
fn context_load_failure_is_internal_exit_1() {
    let mut out = Vec::new();
    let code = run_with(&args(&["agents", "list"]), &mut out, || {
        Err(CoreError::SettingsParse("bad config".into()))
    });
    assert_eq!(code, 1);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(error_code(&v), "INTERNAL");
}

#[test]
fn malformed_suites_file_is_internal_exit_1() {
    let h = home();
    fs::write(h.path().join("suites.json"), "{ not json").unwrap();
    let (code, v, _) = run_in(h.path(), &["agents", "list"]);
    assert_eq!(code, 1);
    assert_eq!(error_code(&v), "INTERNAL");
}
