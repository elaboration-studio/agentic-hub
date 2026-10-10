use std::fs;
use std::path::Path;

use super::*;
use crate::agent_spec::AgentSpec;
use crate::model::{CapabilityKind, SuiteCapabilityRef};
use crate::scanner::scan;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn suite(caps: &[&str]) -> SuiteDefinition {
    SuiteDefinition {
        id: "s1".into(),
        name: "CTO".into(),
        description: Some("delivery".into()),
        capabilities: caps.iter().map(|c| SuiteCapabilityRef::bare(*c)).collect(),
        is_base: false,
        agent: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

/// A source root with one skill (two files) and one rule.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("skills/cto/tdd/SKILL.md"), "# tdd");
    write(&dir.path().join("skills/cto/tdd/refs/a.md"), "ref a");
    write(&dir.path().join("rules/core.md"), "be precise");
    dir
}

fn version_of(root: &Path, s: &SuiteDefinition) -> String {
    agent_version(s, &scan(root).items)
}

const CAPS: [&str; 2] = ["skill:cto/tdd", "rule:core.md"];

#[test]
fn version_is_twelve_lowercase_hex_and_stable_across_runs() {
    let dir = fixture();
    let v1 = version_of(dir.path(), &suite(&CAPS));
    let v2 = version_of(dir.path(), &suite(&CAPS));
    assert_eq!(v1, v2);
    assert_eq!(v1.len(), 12);
    assert!(
        v1.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "{v1}"
    );
}

#[test]
fn version_changes_when_a_rule_is_edited() {
    let dir = fixture();
    let before = version_of(dir.path(), &suite(&CAPS));
    write(&dir.path().join("rules/core.md"), "be very precise");
    assert_ne!(before, version_of(dir.path(), &suite(&CAPS)));
}

#[test]
fn version_changes_when_a_nested_skill_file_is_edited() {
    let dir = fixture();
    let before = version_of(dir.path(), &suite(&CAPS));
    write(
        &dir.path().join("skills/cto/tdd/refs/a.md"),
        "ref a, revised",
    );
    assert_ne!(before, version_of(dir.path(), &suite(&CAPS)));
}

#[test]
fn version_changes_when_a_file_is_added_to_a_skill() {
    let dir = fixture();
    let before = version_of(dir.path(), &suite(&CAPS));
    write(&dir.path().join("skills/cto/tdd/refs/b.md"), "ref b");
    assert_ne!(before, version_of(dir.path(), &suite(&CAPS)));
}

#[test]
fn version_changes_when_a_capability_is_added_or_removed() {
    let dir = fixture();
    let both = version_of(dir.path(), &suite(&CAPS));
    let one = version_of(dir.path(), &suite(&["skill:cto/tdd"]));
    assert_ne!(both, one);
}

#[test]
fn version_changes_when_the_agent_block_is_edited() {
    let dir = fixture();
    let mut s = suite(&CAPS);
    let plain = version_of(dir.path(), &s);
    s.agent = Some(AgentSpec {
        emoji: Some("⚒️".into()),
        ..Default::default()
    });
    let with_agent = version_of(dir.path(), &s);
    s.agent = Some(AgentSpec {
        emoji: Some("🧭".into()),
        ..Default::default()
    });
    assert_ne!(plain, with_agent);
    assert_ne!(with_agent, version_of(dir.path(), &s));
}

#[test]
fn version_changes_when_the_suite_name_changes() {
    let dir = fixture();
    let mut s = suite(&CAPS);
    let before = version_of(dir.path(), &s);
    s.name = "CPO".into();
    assert_ne!(before, version_of(dir.path(), &s));
}

#[test]
fn version_ignores_timestamps_base_flag_and_ref_order() {
    let dir = fixture();
    let before = version_of(dir.path(), &suite(&CAPS));
    let mut s = suite(&["rule:core.md", "skill:cto/tdd"]);
    s.created_at = "2030-01-01T00:00:00Z".into();
    s.updated_at = "2030-02-02T00:00:00Z".into();
    s.is_base = true;
    assert_eq!(before, version_of(dir.path(), &s));
}

#[test]
fn version_changes_when_a_missing_capability_appears() {
    let dir = fixture();
    let caps = ["skill:cto/tdd", "skill:later"];
    let before = version_of(dir.path(), &suite(&caps));
    write(&dir.path().join("skills/later/SKILL.md"), "# later");
    assert_ne!(before, version_of(dir.path(), &suite(&caps)));
}

#[test]
fn missing_capability_hashes_differently_from_no_capability() {
    let dir = fixture();
    let without = version_of(dir.path(), &suite(&["skill:cto/tdd"]));
    let with_missing = version_of(dir.path(), &suite(&["skill:cto/tdd", "skill:gone"]));
    assert_ne!(without, with_missing);
}

#[test]
fn version_ignores_finder_metadata_files() {
    let dir = fixture();
    let before = version_of(dir.path(), &suite(&CAPS));
    write(&dir.path().join("skills/cto/tdd/.DS_Store"), "finder");
    assert_eq!(before, version_of(dir.path(), &suite(&CAPS)));
}

#[cfg(unix)]
#[test]
fn symlink_inside_a_skill_hashes_its_target_string_not_its_content() {
    use std::os::unix::fs::symlink;
    let dir = fixture();
    let outside = dir.path().join("outside.md");
    write(&outside, "v1");
    symlink(&outside, dir.path().join("skills/cto/tdd/linked.md")).unwrap();
    let before = version_of(dir.path(), &suite(&CAPS));

    write(&outside, "v2 — content behind the link changed");
    assert_eq!(
        before,
        version_of(dir.path(), &suite(&CAPS)),
        "not followed"
    );

    fs::remove_file(dir.path().join("skills/cto/tdd/linked.md")).unwrap();
    symlink(
        dir.path().join("elsewhere.md"),
        dir.path().join("skills/cto/tdd/linked.md"),
    )
    .unwrap();
    assert_ne!(
        before,
        version_of(dir.path(), &suite(&CAPS)),
        "target string hashed"
    );
}

#[test]
fn resolve_caps_sorts_dedupes_and_marks_missing() {
    let dir = fixture();
    let items = scan(dir.path()).items;
    let s = suite(&[
        "skill:cto/tdd",
        "rule:core.md",
        "skill:gone",
        "skill:cto/tdd",
    ]);
    let resolved = resolve_caps(&s, &items);
    let ids: Vec<&str> = resolved.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["rule:core.md", "skill:cto/tdd", "skill:gone"]);
    assert_eq!(
        resolved[1].item.map(|i| i.kind),
        Some(CapabilityKind::Skill)
    );
    assert!(resolved[2].item.is_none());
}
