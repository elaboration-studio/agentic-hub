use super::*;
use crate::usage_attribution::AttributionState;
use serde_json::json;

struct TestRepo(PathBuf);

impl TestRepo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("agentic-hub-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn local_skill(root: &Path, tool_dir: &str, name: &str) -> PathBuf {
    let dir = root.join(tool_dir).join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
    dir.join("SKILL.md")
}

#[test]
fn attribute_batch_resolves_slash_repository_skill_for_cursor_claude_and_codex() {
    for (tool, skill_dir) in [
        ("cursor", ".cursor/skills"),
        ("claude", ".claude/skills"),
        ("codex", ".agents/skills"),
    ] {
        let repo = TestRepo::new();
        fs::create_dir(repo.path().join(".git")).unwrap();
        local_skill(repo.path(), skill_dir, "slash-repo-skill");
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "turn_id": "turn-slash",
            "generation_id": "generation-slash",
            "session_id": "session-slash",
            "cwd": repo.path(),
            "workspace_roots": [repo.path()],
            "prompt": "/slash-repo-skill debug this"
        });
        let batch = AttributionState::default().normalize(&raw, tool);

        let attributed = attribute_batch(batch, &Settings::default(), tool);

        assert_eq!(
            attributed.events.len(),
            1,
            "tool={tool} should resolve catalog-known slash skill"
        );
        assert_eq!(
            attributed.events[0].capability_scope,
            CapabilityScope::Workspace,
            "tool={tool}"
        );
        assert_eq!(
            attributed.events[0].skill_name.as_deref(),
            Some("slash-repo-skill"),
            "tool={tool}"
        );
        assert_eq!(
            attributed.events[0].attribution_source.as_deref(),
            Some("slash_reference"),
            "tool={tool}"
        );
    }
}

#[test]
fn attribute_batch_drops_unknown_slash_without_unresolved_noise() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-1",
        "workspace_roots": [repo.path()],
        "prompt": "/health /not-a-real-skill"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert!(attributed.events.is_empty());
}

#[test]
fn codex_prefers_unique_workspace_skill_over_global_name_collision() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    local_skill(repo.path(), ".agents/skills", "shared-leaf-skill");
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "turn_id": "turn-1",
        "cwd": repo.path(),
        "prompt": "/shared-leaf-skill"
    });
    let batch = AttributionState::default().normalize(&raw, "codex");

    // Global catalog may also contain skills; when a unique workspace match
    // exists, Codex now prefers it (same precedence as Cursor).
    let attributed = attribute_batch(batch, &Settings::default(), "codex");

    assert_eq!(attributed.events.len(), 1);
    assert_eq!(
        attributed.events[0].capability_scope,
        CapabilityScope::Workspace
    );
    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("skill:shared-leaf-skill")
    );
}

#[test]
fn attribute_batch_resolves_slash_repository_agent_for_cursor() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let agent_dir = repo.path().join(".cursor/agents");
    fs::create_dir_all(&agent_dir).unwrap();
    let agent_path = agent_dir.join("repo-only-cto.md");
    fs::write(&agent_path, "# repo-only-cto\n").unwrap();
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-agent",
        "workspace_roots": [repo.path()],
        "prompt": "/repo-only-cto investigate"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert_eq!(attributed.events.len(), 1);
    assert_eq!(
        attributed.events[0].capability_scope,
        CapabilityScope::Workspace
    );
    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("agent:repo-only-cto.md")
    );
    assert_eq!(
        attributed.events[0].skill_name.as_deref(),
        Some("repo-only-cto")
    );
}

#[test]
fn attribute_batch_resolves_agent_read_path_in_repository() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let agent_dir = repo.path().join(".cursor/agents");
    fs::create_dir_all(&agent_dir).unwrap();
    let agent_path = agent_dir.join("path-cto.md");
    fs::write(&agent_path, "# path-cto\n").unwrap();
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Read",
        "workspace_roots": [repo.path()],
        "tool_input": { "file_path": agent_path }
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert_eq!(attributed.events.len(), 1);
    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("agent:path-cto.md")
    );
    assert_eq!(
        attributed.events[0].attribution_source.as_deref(),
        Some("agent_read")
    );
}

#[test]
fn attribute_batch_resolves_repository_only_skill() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    local_skill(repo.path(), ".agents/skills", "repo-only-unique-skill");
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "turn_id": "turn-1",
        "cwd": repo.path(),
        "prompt": "$repo-only-unique-skill"
    });
    let batch = AttributionState::default().normalize(&raw, "codex");

    let attributed = attribute_batch(batch, &Settings::default(), "codex");

    assert_eq!(
        attributed.events[0].capability_scope,
        CapabilityScope::Workspace
    );
    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("skill:repo-only-unique-skill")
    );
}

#[test]
fn attribute_batch_drops_generic_slash_command_and_file_paths() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-1",
        "workspace_roots": [repo.path()],
        "prompt": "/health /tmp/pasted-text.txt /tmp/image.png"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert!(attributed.events.is_empty());
}

#[test]
fn unresolved_single_repository_reference_preserves_workspace_for_reconciliation() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-1",
        "workspace_roots": [repo.path()],
        "prompt": "$not-installed-yet"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    let expected = tildify(&repo.path().canonicalize().unwrap());
    assert_eq!(
        attributed.events[0].workspace_root.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(attributed.events[0].capability_id, None);
}

#[test]
fn unresolved_multi_root_reference_does_not_guess_a_workspace() {
    let first = TestRepo::new();
    let second = TestRepo::new();
    fs::create_dir(first.path().join(".git")).unwrap();
    fs::create_dir(second.path().join(".git")).unwrap();
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-1",
        "workspace_roots": [first.path(), second.path()],
        "prompt": "$ambiguous-missing-skill"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert_eq!(attributed.events[0].workspace_root, None);
    assert_eq!(attributed.events[0].capability_id, None);
}

#[test]
fn exact_skill_path_disambiguates_repository_skill() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let skill_path = local_skill(repo.path(), ".claude/skills/nested", "collision-skill");
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "session-1",
        "cwd": repo.path(),
        "prompt": format!("[$collision-skill]({})", skill_path.display())
    });
    let batch = AttributionState::default().normalize(&raw, "claude");

    let attributed = attribute_batch(batch, &Settings::default(), "claude");

    assert_eq!(
        attributed.events[0].capability_relative_path.as_deref(),
        Some("nested/collision-skill")
    );
}

#[test]
fn exact_skill_path_outside_repository_roots_is_rejected() {
    let repo = TestRepo::new();
    let outside = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let skill_path = local_skill(outside.path(), ".claude/skills", "outside-skill");
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "session-1",
        "cwd": repo.path(),
        "prompt": format!("[$outside-skill]({})", skill_path.display())
    });
    let batch = AttributionState::default().normalize(&raw, "claude");

    let attributed = attribute_batch(batch, &Settings::default(), "claude");

    assert!(attributed.events.is_empty());
}

#[cfg(unix)]
#[test]
fn repository_skill_symlink_stays_resolvable_when_target_is_in_root() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let target = repo.path().join(".agents/skills/real-skill");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "# linked-skill\n").unwrap();
    let linked = repo.path().join(".agents/skills/linked-skill");
    symlink(&target, &linked).unwrap();
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "turn_id": "turn-1",
        "cwd": repo.path(),
        "prompt": format!("[$linked-skill]({})", linked.join("SKILL.md").display())
    });
    let batch = AttributionState::default().normalize(&raw, "codex");

    let attributed = attribute_batch(batch, &Settings::default(), "codex");

    assert_eq!(attributed.events.len(), 1);
    assert_eq!(
        attributed.events[0].capability_scope,
        CapabilityScope::Workspace
    );
}

#[test]
fn repeated_references_share_one_attributed_occurrence() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    local_skill(repo.path(), ".cursor/skills", "repeat-local-skill");
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "generation_id": "generation-1",
        "workspace_roots": [repo.path()],
        "prompt": "$repeat-local-skill $repeat-local-skill"
    });
    let batch = AttributionState::default().normalize(&raw, "cursor");

    let attributed = attribute_batch(batch, &Settings::default(), "cursor");

    assert_eq!(attributed.events.len(), 1);
}

#[test]
fn reconcile_existing_usage_resolves_unique_repository_history() {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    local_skill(
        repo.path(),
        ".cursor/skills",
        "reconcile-unique-local-skill",
    );
    let store = UsageStore::with_path(repo.path().join("trace.db"));
    let unresolved = UsageEventInput {
        source_tool: "cursor".to_string(),
        event_type: "PostSkillUse".to_string(),
        skill_name: Some("reconcile-unique-local-skill".to_string()),
        workspace: Some(repo.path().to_string_lossy().into_owned()),
        dedupe_hash: Some("legacy-local-row".to_string()),
        ..UsageEventInput::default()
    };
    store.insert_event(&unresolved, &[]).unwrap();

    let updated = reconcile_existing_usage(&store, &Settings::default()).unwrap();

    assert_eq!((updated, store.resolved_event_count().unwrap()), (1, 1));
}
