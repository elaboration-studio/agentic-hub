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

fn claude_only_settings(shared_root: &Path, tools_root: &Path) -> Settings {
    isolated_settings(&[ToolId::Claude], shared_root, tools_root)
}

/// Settings pinned entirely inside temp dirs: only `tools` stay enabled, and
/// each one's paths live under `tools_root/<tool>` so the real `~/.claude`,
/// `~/.cursor`, and `~/.agents` trees never leak into a test catalog.
fn isolated_settings(tools: &[ToolId], shared_root: &Path, tools_root: &Path) -> Settings {
    let mut settings = Settings {
        shared_root: shared_root.to_path_buf(),
        sources: Vec::new(),
        ..Settings::default()
    };
    for tool in ToolId::ALL {
        let root = tools_root.join(tool.as_str());
        let enabled = tools.contains(&tool);
        let t = &mut settings.tools;
        let tool_settings = match tool {
            ToolId::Codex => &mut t.codex,
            ToolId::Claude => &mut t.claude,
            ToolId::Cursor => &mut t.cursor,
            ToolId::Openclaw => &mut t.openclaw,
            ToolId::Openstandard => &mut t.openstandard,
            ToolId::Kiro => &mut t.kiro,
            ToolId::Copilot => &mut t.copilot,
            ToolId::Antigravity => &mut t.antigravity,
        };
        tool_settings.enabled = enabled;
        tool_settings.skills_path = root.join("skills");
        tool_settings.agents_path = root.join("agents");
        tool_settings.rules_path = root.join("rules");
        tool_settings.instructions_path = Some(root.join("INSTRUCTIONS.md"));
        tool_settings.hooks_file = Some(root.join("hooks.json"));
        tool_settings.commands_path = Some(root.join("commands"));
    }
    settings
}

fn add_source(settings: &mut Settings, label: &str, path: &Path) {
    settings.sources.push(agentic_core::settings::SourceConfig {
        id: String::new(),
        label: label.to_string(),
        path: path.to_path_buf(),
    });
}

fn slash_prompt_event(prompt: &str, session: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": session,
        "turn_id": session,
        "generation_id": session,
        "prompt": prompt
    })
}

fn skill_tool_event(skill: &str, session: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUse",
        "session_id": session,
        "tool_name": "Skill",
        "tool_input": { "skill": skill }
    })
}

#[test]
fn claude_managed_copy_resolves_to_configured_source_identity() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let settings = claude_only_settings(source.path(), tools.path());
    let source_file = local_skill(source.path(), "skills", "managed-global-skill");
    let target_root = &settings.tools.claude.skills_path;
    agentic_core::managed_copy::write_managed_copy(
        source_file.parent().unwrap(),
        &target_root.join("managed-global-skill"),
        target_root,
        "skill:managed-global-skill",
        false,
    )
    .unwrap();
    let batch = AttributionState::default().normalize(
        &skill_tool_event("managed-global-skill", "s-managed"),
        "claude",
    );

    let attributed = attribute_batch(batch, &settings, "claude");

    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("skill:managed-global-skill")
    );
}

#[test]
fn claude_managed_copy_resolves_after_source_folder_moved() {
    // The manifest records the item id the copy was made from. When the user
    // reorganizes their source forest that id goes stale, but the projection is
    // still a projection — attribution must not fall back to "ambiguous".
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let settings = claude_only_settings(source.path(), tools.path());
    let source_file = local_skill(source.path(), "skills/cto", "moved-skill");
    let target_root = &settings.tools.claude.skills_path;
    agentic_core::managed_copy::write_managed_copy(
        source_file.parent().unwrap(),
        &target_root.join("moved-skill"),
        target_root,
        "skill:legacy/cto/moved-skill",
        false,
    )
    .unwrap();
    let batch = AttributionState::default()
        .normalize(&skill_tool_event("moved-skill", "s-moved"), "claude");

    let attributed = attribute_batch(batch, &settings, "claude");

    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("skill:cto/moved-skill")
    );
}

#[test]
fn unmanaged_copy_beside_a_configured_source_resolves_to_that_source() {
    // Codex-style: `~/.agents/skills/<name>` is a plain copy with no managed
    // manifest. Without treating it as this tool's projection it becomes a
    // second same-name candidate and the invocation is discarded.
    for tool in [ToolId::Codex, ToolId::Cursor, ToolId::Claude] {
        let source = TestRepo::new();
        let tools = TestRepo::new();
        let settings = isolated_settings(&[tool], source.path(), tools.path());
        local_skill(source.path(), "skills", "copied-skill");
        local_skill(
            tools.path(),
            &format!("{}/skills", tool.as_str()),
            "copied-skill",
        );
        let batch = AttributionState::default()
            .normalize(&skill_tool_event("copied-skill", "s-copy"), tool.as_str());

        let attributed = attribute_batch(batch, &settings, tool.as_str());

        assert_eq!(
            attributed.events[0].capability_id.as_deref(),
            Some("skill:copied-skill"),
            "tool={tool:?}"
        );
    }
}

#[test]
fn projection_breaks_a_tie_between_two_sources_sharing_a_leaf_name() {
    // Two configured sources both expose `shared-name`. The tool only projects
    // one of them, and that projection is the ground truth for what ran.
    for tool in [ToolId::Cursor, ToolId::Codex, ToolId::Claude] {
        let primary = TestRepo::new();
        let secondary = TestRepo::new();
        let tools = TestRepo::new();
        let mut settings = isolated_settings(&[tool], primary.path(), tools.path());
        add_source(&mut settings, "primary", primary.path());
        add_source(&mut settings, "secondary", secondary.path());
        let winner = local_skill(primary.path(), "skills/cmo", "shared-name");
        local_skill(secondary.path(), "skills/cto/web", "shared-name");
        let projection_root = tools.path().join(tool.as_str()).join("skills");
        fs::create_dir_all(&projection_root).unwrap();
        std::os::unix::fs::symlink(
            winner.parent().unwrap(),
            projection_root.join("shared-name"),
        )
        .unwrap();
        let batch = AttributionState::default()
            .normalize(&skill_tool_event("shared-name", "s-tie"), tool.as_str());

        let attributed = attribute_batch(batch, &settings, tool.as_str());

        assert_eq!(
            attributed.events[0].capability_id.as_deref(),
            Some("skill:cmo/shared-name"),
            "tool={tool:?} must follow its own projection"
        );
    }
}

#[test]
fn content_hash_breaks_a_tie_when_the_manifest_source_moved() {
    // Worst case seen in the wild: a hard copy whose manifest source path no
    // longer exists, plus two sources sharing the name. The recorded content
    // hash still identifies which one it was copied from.
    let primary = TestRepo::new();
    let secondary = TestRepo::new();
    let gone = TestRepo::new();
    let tools = TestRepo::new();
    let mut settings = isolated_settings(&[ToolId::Claude], primary.path(), tools.path());
    add_source(&mut settings, "primary", primary.path());
    add_source(&mut settings, "secondary", secondary.path());
    let origin = local_skill(gone.path(), "skills", "drifted-skill");
    // The surviving copy of that exact content now lives in `secondary`.
    let twin = local_skill(secondary.path(), "skills/cto", "drifted-skill");
    fs::write(&twin, fs::read_to_string(&origin).unwrap()).unwrap();
    // `primary` exposes the same name with different content.
    let other = local_skill(primary.path(), "skills/cmo", "drifted-skill");
    fs::write(&other, "# a different drifted-skill\n").unwrap();
    let target_root = &settings.tools.claude.skills_path;
    agentic_core::managed_copy::write_managed_copy(
        origin.parent().unwrap(),
        &target_root.join("drifted-skill"),
        target_root,
        "skill:drifted-skill",
        false,
    )
    .unwrap();
    drop(gone); // the recorded source path disappears
    let batch = AttributionState::default()
        .normalize(&skill_tool_event("drifted-skill", "s-drift"), "claude");

    let attributed = attribute_batch(batch, &settings, "claude");

    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("skill:cto/drifted-skill")
    );
}

#[test]
fn ambiguous_known_skill_is_kept_unresolved_instead_of_dropped() {
    // Genuinely undecidable: two sources, no projection to break the tie. The
    // invocation must still be counted so reconciliation can repair it later.
    let primary = TestRepo::new();
    let secondary = TestRepo::new();
    let tools = TestRepo::new();
    let mut settings = isolated_settings(&[ToolId::Cursor], primary.path(), tools.path());
    add_source(&mut settings, "primary", primary.path());
    add_source(&mut settings, "secondary", secondary.path());
    local_skill(primary.path(), "skills/cmo", "tied-name");
    local_skill(secondary.path(), "skills/cto", "tied-name");
    let batch = AttributionState::default().normalize(
        &slash_prompt_event("/tied-name ship it", "s-ambiguous"),
        "cursor",
    );

    let attributed = attribute_batch(batch, &settings, "cursor");

    assert_eq!(attributed.events.len(), 1, "invocation must not be lost");
    assert_eq!(
        attributed.events[0].skill_name.as_deref(),
        Some("tied-name")
    );
    assert!(attributed.events[0].capability_id.is_none());
}

#[test]
fn mid_prompt_slash_reference_is_tracked_identically_for_every_tool() {
    // The `/hotfix` report: the slash reference sat at the end of a sentence,
    // and the skill existed as both a configured source and a tool projection.
    let mut ids = Vec::new();
    for tool in [ToolId::Cursor, ToolId::Codex, ToolId::Claude] {
        let source = TestRepo::new();
        let tools = TestRepo::new();
        let settings = isolated_settings(&[tool], source.path(), tools.path());
        local_skill(source.path(), "skills/cto", "hotfix");
        local_skill(tools.path(), &format!("{}/skills", tool.as_str()), "hotfix");
        let batch = AttributionState::default().normalize(
            &slash_prompt_event(
                "good re-trigger the build for vercel to ship this hotfix /hotfix",
                "s-mid",
            ),
            tool.as_str(),
        );

        let attributed = attribute_batch(batch, &settings, tool.as_str());

        assert_eq!(attributed.events.len(), 1, "tool={tool:?}");
        ids.push(attributed.events[0].capability_id.clone());
    }
    assert_eq!(
        ids,
        vec![Some("skill:cto/hotfix".to_string()); 3],
        "codex, cursor and claude must agree"
    );
}

#[test]
fn claude_unmanaged_skill_keeps_installed_identity() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let settings = claude_only_settings(source.path(), tools.path());
    local_skill(tools.path(), "claude/skills", "unmanaged-global-skill");
    let batch = AttributionState::default().normalize(
        &skill_tool_event("unmanaged-global-skill", "s-unmanaged"),
        "claude",
    );

    let attributed = attribute_batch(batch, &settings, "claude");

    assert_eq!(
        attributed.events[0].capability_id.as_deref(),
        Some("installed::claude::skill:unmanaged-global-skill")
    );
}

#[test]
fn reconciliation_repairs_claude_managed_copy_history() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let workspace = TestRepo::new();
    let settings = claude_only_settings(source.path(), tools.path());
    let source_file = local_skill(source.path(), "skills", "managed-history-skill");
    let target_root = &settings.tools.claude.skills_path;
    agentic_core::managed_copy::write_managed_copy(
        source_file.parent().unwrap(),
        &target_root.join("managed-history-skill"),
        target_root,
        "skill:managed-history-skill",
        false,
    )
    .unwrap();
    let store = UsageStore::with_path(tools.path().join("trace.db"));
    let unresolved = UsageEventInput {
        source_tool: "claude".to_string(),
        event_type: "PostToolUse".to_string(),
        skill_name: Some("managed-history-skill".to_string()),
        workspace: Some(workspace.path().to_string_lossy().into_owned()),
        dedupe_hash: Some("claude-managed-history".to_string()),
        ..UsageEventInput::default()
    };
    store.insert_event(&unresolved, &[]).unwrap();

    let updated = reconcile_existing_usage(&store, &settings).unwrap();

    assert_eq!((updated, store.resolved_event_count().unwrap()), (1, 1));
}

#[test]
fn reconciliation_repairs_global_scope_history_without_workspace_root() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let settings = claude_only_settings(source.path(), tools.path());
    let source_file = local_skill(source.path(), "skills", "global-history-skill");
    let target_root = &settings.tools.claude.skills_path;
    agentic_core::managed_copy::write_managed_copy(
        source_file.parent().unwrap(),
        &target_root.join("global-history-skill"),
        target_root,
        "skill:global-history-skill",
        false,
    )
    .unwrap();
    let store = UsageStore::with_path(tools.path().join("trace.db"));
    let unresolved = UsageEventInput {
        source_tool: "claude".to_string(),
        event_type: "PostToolUse".to_string(),
        skill_name: Some("global-history-skill".to_string()),
        dedupe_hash: Some("claude-global-history".to_string()),
        ..UsageEventInput::default()
    };
    store.insert_event(&unresolved, &[]).unwrap();

    let updated = reconcile_existing_usage(&store, &settings).unwrap();

    assert_eq!((updated, store.resolved_event_count().unwrap()), (1, 1));
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

fn skill_with_frontmatter(root: &Path, folder: &str, name: &str) -> PathBuf {
    let dir = root.join("skills").join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: test\n---\n# {name}\n"),
    )
    .unwrap();
    dir.join("SKILL.md")
}

#[test]
fn codex_resolves_frontmatter_alias_and_spaced_slash_to_folder_skill() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let settings = isolated_settings(&[ToolId::Codex], source.path(), tools.path());
    skill_with_frontmatter(source.path(), "grill-me", "grilling");
    let batch = AttributionState::default().normalize(
        &slash_prompt_event(
            "/root-cause-investigation /grill me about this plan",
            "s-grill",
        ),
        "codex",
    );
    let alias =
        AttributionState::default().normalize(&skill_tool_event("grilling", "s-alias"), "codex");

    let spaced = attribute_batch(batch, &settings, "codex");
    let named = attribute_batch(alias, &settings, "codex");

    assert_eq!(
        spaced
            .events
            .iter()
            .filter_map(|event| event.capability_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["skill:grill-me"]
    );
    assert_eq!(
        named.events[0].capability_id.as_deref(),
        Some("skill:grill-me")
    );
}

#[test]
fn kiro_resolves_repository_and_global_skills() {
    let source = TestRepo::new();
    let tools = TestRepo::new();
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let mut settings = isolated_settings(&[ToolId::Kiro], source.path(), tools.path());
    settings.tools.kiro.hooks_dir = Some(tools.path().join("kiro/hooks"));
    local_skill(source.path(), "skills/cto", "repo-research");
    local_skill(repo.path(), ".kiro/skills", "local-kiro-skill");
    let global = AttributionState::default().normalize(
        &slash_prompt_event("/repo-research what is this?", "s-kiro"),
        "kiro",
    );
    let local = AttributionState::default().normalize(
        &json!({
            "hook_event_name": "userPromptSubmit",
            "session_id": "s-kiro-local",
            "cwd": repo.path(),
            "prompt": "/local-kiro-skill help"
        }),
        "kiro",
    );

    let global_attr = attribute_batch(global, &settings, "kiro");
    let local_attr = attribute_batch(local, &settings, "kiro");

    assert_eq!(
        global_attr.events[0].capability_id.as_deref(),
        Some("skill:cto/repo-research")
    );
    assert_eq!(
        local_attr.events[0].capability_id.as_deref(),
        Some("skill:local-kiro-skill")
    );
}
