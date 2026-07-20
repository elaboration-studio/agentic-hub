use super::*;

#[test]
fn normalize_event_reads_explicit_skill_name_without_arguments() {
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Skill",
        "tool_input": {
            "skill_name": "root-cause-investigation",
            "prompt": "do not store"
        },
        "model": "test-model"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.source_tool, "claude");
    assert_eq!(input.metadata["model"], "test-model");
    assert!(input.metadata.get("prompt").is_none());
}

#[test]
fn normalize_event_canonicalizes_cursor_post_tool_use() {
    let raw = json!({
        "event_type": "postToolUse",
        "tool_name": "Read"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.source_tool, "cursor");
    assert_eq!(input.tool_name.as_deref(), Some("Read"));
}

#[test]
fn normalize_event_extracts_explicit_prompt_skill_ref_without_storing_prompt() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "[$root-cause-investigation](/Users/ArnoYe/.agentic-arno/skills/arno/cto/root-cause-investigation/SKILL.md) debug this",
        "model": "cursor-test"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.metadata["model"], "cursor-test");
    assert!(input.metadata.get("prompt").is_none());
}

#[test]
fn normalize_event_extracts_slash_skill_from_claude_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/root-cause-investigation why is usage not tracked?",
        "model": "claude-sonnet-5"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_extracts_skill_from_claude_prompt_expansion() {
    let raw = json!({
        "hook_event_name": "UserPromptExpansion",
        "expansion_type": "slash_command",
        "command_name": "root-cause-investigation",
        "command_args": "help me debug this",
        "prompt": "/root-cause-investigation help me debug this",
        "model": "claude-sonnet-5"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.source_tool, "claude");
    assert!(input.metadata.get("prompt").is_none());
}

#[test]
fn normalize_event_ignores_non_skill_claude_prompt_expansion() {
    let raw = json!({
        "hook_event_name": "UserPromptExpansion",
        "expansion_type": "mcp_prompt",
        "command_name": "some-mcp-prompt",
        "prompt": "/some-mcp-prompt"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(input.event_type, "UserPromptExpansion");
    assert_eq!(input.skill_name, None);
}

#[test]
fn normalize_event_extracts_dollar_skill_from_codex_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "$root-cause-investigation debug this",
        "model": "gpt-5.3-codex"
    });

    let input = normalize_event(raw, "codex");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.metadata["model"], "gpt-5.3-codex");
    assert!(input.metadata.get("prompt").is_none());
}

#[test]
fn normalize_event_extracts_slash_agent_from_cursor_prompt_submit() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "/cto investigate this bug with root-cause-investigation",
        "model": "claude-sonnet"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
}

#[test]
fn normalize_event_extracts_at_agent_from_claude_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "@agent-cto investigate this bug",
        "model": "claude-sonnet-5"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
}

#[test]
fn normalize_event_extracts_agent_from_read_tool() {
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Read",
        "tool_input": {
            "file_path": "/Users/me/.cursor/agents/cto.md"
        }
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
}

#[test]
fn normalize_event_keeps_ambiguous_slash_prompt_refs_unresolved() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "/cto and /ceo review this"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "UserPromptSubmit");
    assert_eq!(input.skill_name, None);
}
#[test]
fn normalize_event_extracts_slash_skill_from_codex_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/root-cause-investigation why is usage not tracked?",
        "model": "gpt-5.5"
    });

    let input = normalize_event(raw, "codex");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_extracts_markdown_skill_from_codex_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "[$root-cause-investigation](/Users/ArnoYe/.agentic-arno/skills/arno/cto/root-cause-investigation/SKILL.md) debug this",
        "model": "gpt-5.5"
    });

    let input = normalize_event(raw, "codex");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_keeps_ambiguous_codex_prompt_refs_unresolved() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "$root-cause-investigation and $security-review"
    });

    let input = normalize_event(raw, "codex");

    assert_eq!(input.event_type, "UserPromptSubmit");
    assert_eq!(input.skill_name, None);
}

#[test]
fn normalize_event_extracts_skill_from_claude_skill_tool_input() {
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Skill",
        "tool_input": { "skill": "helper-gitlab" },
        "model": "haiku"
    });

    let input = normalize_event(raw, "claude");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("helper-gitlab"));
    assert_eq!(input.source_tool, "claude");
}

#[test]
fn normalize_event_extracts_skill_from_codex_skill_tool_input() {
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Skill",
        "tool_input": { "skill": "helper-gitlab" },
        "model": "gpt-5.3-codex"
    });

    let input = normalize_event(raw, "codex");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("helper-gitlab"));
    assert_eq!(input.source_tool, "codex");
}

#[test]
fn normalize_event_keeps_ambiguous_prompt_refs_unresolved() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "$root-cause-investigation and $security-review"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "UserPromptSubmit");
    assert_eq!(input.skill_name, None);
}

#[test]
fn normalize_event_extracts_skill_from_cursor_skill_tool_input() {
    let raw = json!({
        "event_type": "postToolUse",
        "tool_name": "Skill",
        "tool_input": { "skill": "helper-gitlab" },
        "model": "composer-2.5"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("helper-gitlab"));
}

#[test]
fn normalize_event_extracts_single_attached_skill_from_before_submit_prompt() {
    let raw = json!({
        "hook_event_name": "beforeSubmitPrompt",
        "prompt": "what can helper-gitlab do?",
        "attachments": [
            {
                "type": "file",
                "file_path": "/Users/ArnoYe/.helper/skills/helper-gitlab/SKILL.md"
            }
        ],
        "model": "composer-2.5"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("helper-gitlab"));
}

#[test]
fn normalize_event_extracts_slash_skill_from_cursor_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/root-cause-investigation why is usage not tracked?",
        "model": "composer-2.5"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_extracts_skill_from_read_tool_path() {
    let raw = json!({
        "event_type": "postToolUse",
        "tool_name": "Read",
        "tool_input": {
            "path": "/Users/ArnoYe/.agents/skills/arno/cto/root-cause-investigation/SKILL.md"
        },
        "model": "composer-2.5"
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn sync_tracer_hooks_writes_cursor_hooks_file() {
    let settings = Settings::load().expect("settings");
    assert!(
        settings.usage_tracing.enabled,
        "usage tracing must be enabled in ~/.agentic-hub/config.json"
    );
    sync_tracer_hooks(&settings).expect("sync tracer hooks");
    let path = settings
        .tools
        .cursor
        .hooks_file
        .as_ref()
        .expect("cursor hooks path");
    assert!(
        path.exists(),
        "cursor hooks file should exist at {}",
        path.display()
    );
}

#[test]
fn normalize_event_keeps_ambiguous_attached_skills_unresolved() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "diagnose this",
        "attachments": [
            {
                "type": "file",
                "file_path": "/Users/ArnoYe/.helper/skills/helper-gitlab/SKILL.md"
            },
            {
                "type": "file",
                "file_path": "/Users/ArnoYe/.agentic-arno/skills/arno/cto/root-cause-investigation/SKILL.md"
            }
        ]
    });

    let input = normalize_event(raw, "cursor");

    assert_eq!(input.event_type, "UserPromptSubmit");
    assert_eq!(input.skill_name, None);
}
