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
