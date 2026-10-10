use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::test_fixture::*;
use super::*;

fn json_at(path: &Path) -> Value {
    serde_json::from_str(&read(path)).unwrap()
}

#[test]
fn cursor_bundle_is_a_plugin_with_every_kind_and_a_manifest() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);

    let root = f.bundles_root().join(CTO).join(&m.version).join("cursor");
    let plugin = root.join("plugin");
    assert_eq!(m.root, root);
    assert_eq!(m.mount, Mount::Plugin);
    assert_eq!(m.plugin_dir.as_deref(), Some(plugin.as_path()));
    assert_eq!(read(&plugin.join("skills/tdd/SKILL.md")), "# cto tdd");
    assert_eq!(read(&plugin.join("skills/tdd/scripts/run.sh")), "echo run");
    assert_eq!(read(&plugin.join("agents/reviewer.md")), "# reviewer");
    assert_eq!(read(&plugin.join("commands/ship.md")), "# ship");
    assert!(plugin.join("hooks/hooks.json").is_file());
    assert!(!plugin.join(".claude-plugin").exists());
    let on_disk: BundleManifest = serde_json::from_str(&read(&root.join(MANIFEST_FILE))).unwrap();
    assert_eq!(on_disk, m);
}

#[test]
fn plugin_json_names_the_slugified_suite_at_the_agent_version() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    let plugin_json = json_at(&m.root.join("plugin/.cursor-plugin/plugin.json"));
    assert_eq!(plugin_json["name"], "ehub-arno-s-cto");
    assert_eq!(plugin_json["version"], m.version.as_str());
    assert_eq!(plugin_json["description"], "delivery");
}

#[test]
fn claude_bundle_uses_the_claude_plugin_dir() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Claude);
    assert!(m.root.join("plugin/.claude-plugin/plugin.json").is_file());
    assert!(!m.root.join("plugin/.cursor-plugin").exists());
    assert_eq!(m.harness, Harness::Claude);
}

#[test]
fn prompt_bundles_carry_skills_only_and_skip_hooks_agents_commands() {
    let f = Fixture::new();
    for harness in [Harness::Codex, Harness::Grok] {
        let m = f.build(CTO, harness);
        assert_eq!(m.mount, Mount::Prompt);
        assert_eq!(m.plugin_dir, None);
        assert!(!m.root.join("plugin").exists());
        assert_eq!(read(&m.root.join("skills/tdd/SKILL.md")), "# cto tdd");
        assert_eq!(
            skipped(&m, "hook:audit"),
            vec!["prompt mount has no hook support"]
        );
        assert_eq!(
            skipped(&m, "agent:reviewer.md"),
            vec!["prompt mount has no agent support"]
        );
        assert_eq!(
            skipped(&m, "command:ship.md"),
            vec!["prompt mount has no command support"]
        );
    }
}

#[test]
fn skills_list_names_each_copied_skill_by_leaf() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Codex);
    assert_eq!(
        m.skills,
        vec![BundleSkill {
            name: "tdd".into(),
            path: m.root.join("skills/tdd"),
        }]
    );
}

#[test]
fn instructions_are_agent_text_then_rules_in_id_order() {
    let f = Fixture::new();
    for harness in Harness::ALL {
        let m = f.build(CTO, harness);
        let file = m.instructions_file.clone().unwrap();
        assert_eq!(file, m.root.join("instructions.md"));
        assert_eq!(
            read(&file),
            "Lead.\n\n<!-- rule:core.md -->\nBe precise.\n\n<!-- rule:style.md -->\nShort words.\n"
        );
    }
}

#[test]
fn instructions_file_is_null_without_instructions_or_rules() {
    let f = Fixture::new();
    let m = f.build(BARE, Harness::Cursor);
    assert_eq!(m.instructions_file, None);
    assert!(!m.root.join("instructions.md").exists());
}

#[test]
fn leaf_collision_keeps_first_id_and_skips_the_rest() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    assert_eq!(skipped(&m, "skill:dev/tdd"), vec!["name collision"]);
    assert_eq!(
        read(&m.root.join("plugin/skills/tdd/SKILL.md")),
        "# cto tdd"
    );
}

#[test]
fn cursor_hooks_are_translated_with_the_absolute_copied_dir() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    let hook_dir = m.root.join("plugin/hooks/audit");
    assert_eq!(read(&hook_dir.join("audit.sh")), "echo audit");
    let command = format!("{}/audit.sh", hook_dir.display());
    assert_eq!(
        json_at(&m.root.join("plugin/hooks/hooks.json")),
        json!({ "version": 1, "hooks": { "preToolUse": [
            { "command": command, "matcher": "Bash", "timeout": 5 }
        ] } })
    );
    assert_eq!(
        skipped(&m, "hook:audit"),
        vec!["Notification is not supported by Cursor; entry skipped."]
    );
}

#[test]
fn claude_hooks_use_the_grouped_shape_without_hub_markers() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Claude);
    let command = format!("{}/audit.sh", m.root.join("plugin/hooks/audit").display());
    let entry = json!({ "type": "command", "command": command, "timeout": 5 });
    assert_eq!(
        json_at(&m.root.join("plugin/hooks/hooks.json")),
        json!({ "hooks": {
            "PreToolUse": [{ "matcher": "Bash", "hooks": [entry] }],
            "Notification": [{ "hooks": [entry] }]
        } })
    );
    assert!(skipped(&m, "hook:audit").is_empty());
}

#[test]
fn hook_that_does_not_target_the_harness_is_skipped() {
    let mut f = Fixture::new();
    write(
        &f.src().join("hooks/audit/hook.json"),
        r#"{ "id": "audit", "command": "x", "targets": ["codex"], "events": [{ "name": "Stop" }] }"#,
    );
    f.rescan();
    let m = f.build(CTO, Harness::Cursor);
    assert_eq!(
        skipped(&m, "hook:audit"),
        vec!["hook does not target cursor"]
    );
    assert!(!m.root.join("plugin/hooks").exists());
}

#[test]
fn mcp_servers_are_listed_for_every_harness_with_env_names_only() {
    let f = Fixture::new();
    for harness in Harness::ALL {
        let m = f.build(CTO, harness);
        let servers = serde_json::to_value(&m.mcp_servers).unwrap();
        assert_eq!(
            servers,
            json!([{ "name": "github", "transport": "stdio", "command": "github-mcp",
                     "args": ["stdio"], "envNames": ["GITHUB_TOKEN"] }])
        );
    }
}

#[test]
fn invalid_mcp_manifest_is_skipped_with_its_error() {
    let mut f = Fixture::new();
    write(
        &f.src().join("mcp/github/mcp.json"),
        r#"{ "name": "github", "transport": "http" }"#,
    );
    f.rescan();
    let m = f.build(CTO, Harness::Cursor);
    assert!(m.mcp_servers.is_empty());
    assert_eq!(
        skipped(&m, "mcp:github"),
        vec!["http transport requires an http(s) url"]
    );
}

#[test]
fn required_clis_report_the_probe_result_in_order() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    assert_eq!(
        m.required_clis,
        vec![
            RequiredCli {
                id: "gh".into(),
                installed: true
            },
            RequiredCli {
                id: "nope".into(),
                installed: false
            },
        ]
    );
}

#[test]
fn missing_capability_is_skipped_as_not_found() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    assert_eq!(skipped(&m, "skill:gone"), vec!["not found"]);
}

#[test]
fn base_suite_capabilities_are_not_part_of_an_agent_bundle() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    assert!(!m.root.join("plugin/skills/review").exists());
    assert!(m.skills.iter().all(|s| s.name != "review"));
}

#[test]
fn every_manifest_path_is_absolute_and_inside_root() {
    let f = Fixture::new();
    let m = f.build(CTO, Harness::Cursor);
    assert!(m.root.is_absolute());
    let mut paths = vec![
        m.plugin_dir.clone().unwrap(),
        m.instructions_file.clone().unwrap(),
    ];
    paths.extend(m.skills.iter().map(|s| s.path.clone()));
    for p in paths {
        assert!(p.is_absolute() && p.starts_with(&m.root), "{}", p.display());
        assert!(p.exists(), "{}", p.display());
    }
}

#[test]
fn bundle_is_a_copy_that_later_source_edits_cannot_change() {
    let mut f = Fixture::new();
    let first = f.build(CTO, Harness::Cursor);
    let copied = first.root.join("plugin/skills/tdd/SKILL.md");
    assert!(!fs::symlink_metadata(&copied)
        .unwrap()
        .file_type()
        .is_symlink());

    write(&f.src().join("skills/cto/tdd/SKILL.md"), "# edited");
    f.rescan();
    let second = f.build(CTO, Harness::Cursor);
    assert_ne!(first.version, second.version);
    assert_eq!(read(&copied), "# cto tdd");
    assert_eq!(
        read(&second.root.join("plugin/skills/tdd/SKILL.md")),
        "# edited"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_inside_a_skill_copy_file_targets_and_drop_dir_targets() {
    use std::os::unix::fs::symlink;
    let mut f = Fixture::new();
    write(&f.dir.path().join("shared/note.md"), "shared note");
    let skill = f.src().join("skills/cto/tdd");
    symlink(f.dir.path().join("shared/note.md"), skill.join("note.md")).unwrap();
    symlink(f.dir.path().join("shared"), skill.join("shared-dir")).unwrap();
    f.rescan();

    let m = f.build(CTO, Harness::Codex);
    let copied = m.root.join("skills/tdd/note.md");
    assert!(!fs::symlink_metadata(&copied)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(&copied), "shared note");
    assert!(!m.root.join("skills/tdd/shared-dir").exists());
}

#[test]
fn harness_parses_only_the_four_contract_names() {
    assert_eq!(Harness::parse("cursor"), Some(Harness::Cursor));
    assert_eq!(Harness::parse("grok"), Some(Harness::Grok));
    assert_eq!(Harness::parse("Cursor"), None);
    assert_eq!(Harness::parse("kiro"), None);
}

#[test]
fn suite_id_validation_follows_the_contract_pattern() {
    assert!(is_valid_suite_id("3f1c2a9e-0000-4000-8000-000000000000"));
    assert!(is_valid_suite_id("a"));
    assert!(is_valid_suite_id(&format!("a{}", "b".repeat(63))));
    assert!(!is_valid_suite_id(&format!("a{}", "b".repeat(64))));
    assert!(!is_valid_suite_id(""));
    assert!(!is_valid_suite_id(".hidden"));
    assert!(!is_valid_suite_id("../etc"));
    assert!(!is_valid_suite_id("a/b"));
    assert!(!is_valid_suite_id("a b"));
}
