use super::*;
use crate::usage_attribution::AttributionState;

fn first_event(raw: Value, source_tool: &str) -> agentic_core::UsageEventInput {
    let batch = AttributionState::default().normalize(&raw, source_tool);
    batch
        .occurrences
        .into_iter()
        .next()
        .map(|occurrence| occurrence.event)
        .unwrap_or_else(|| agentic_core::UsageEventInput {
            source_tool: source_tool.to_string(),
            event_type: "UserPromptSubmit".to_string(),
            ..agentic_core::UsageEventInput::default()
        })
}

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

    let input = first_event(raw, "claude");

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
        "tool_name": "Read",
        "tool_input": {
            "path": "/Users/ArnoYe/.agents/skills/feature-dev/SKILL.md"
        }
    });

    let input = first_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.source_tool, "cursor");
    assert_eq!(input.tool_name.as_deref(), Some("Read"));
    assert_eq!(input.skill_name.as_deref(), Some("feature-dev"));
}

#[test]
fn normalize_event_extracts_explicit_prompt_skill_ref_without_storing_prompt() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "[$root-cause-investigation](/Users/ArnoYe/.agentic-arno/skills/arno/cto/root-cause-investigation/SKILL.md) debug this",
        "model": "cursor-test"
    });

    let input = first_event(raw, "cursor");

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

    let input = first_event(raw, "claude");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.attribution_source.as_deref(), Some("slash_reference"));
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

    let input = first_event(raw, "claude");

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

    let batch = AttributionState::default().normalize(&raw, "claude");

    assert!(batch.occurrences.is_empty());
}

#[test]
fn normalize_event_extracts_dollar_skill_from_codex_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "$root-cause-investigation debug this",
        "model": "gpt-5.3-codex"
    });

    let input = first_event(raw, "codex");

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

    let input = first_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
    assert_eq!(input.attribution_source.as_deref(), Some("slash_reference"));
}

#[test]
fn normalize_event_extracts_at_agent_from_claude_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "@agent-cto investigate this bug",
        "model": "claude-sonnet-5"
    });

    let input = first_event(raw, "claude");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
    assert_eq!(input.attribution_source.as_deref(), Some("agent_mention"));
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

    let input = first_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("cto"));
    assert_eq!(input.attribution_source.as_deref(), Some("agent_read"));
}

#[test]
fn normalize_event_extracts_multiple_slash_refs() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "/cto and /ceo review this"
    });

    let batch = AttributionState::default().normalize(&raw, "cursor");
    let mut names: Vec<_> = batch
        .occurrences
        .iter()
        .filter_map(|item| item.event.skill_name.clone())
        .collect();
    names.sort();
    names.dedup();

    assert!(names.contains(&"ceo".to_string()), "{names:?}");
    assert!(names.contains(&"cto".to_string()), "{names:?}");
    assert!(batch
        .occurrences
        .iter()
        .all(|item| item.requires_catalog_match));
}

#[test]
fn normalize_event_extracts_slash_skill_from_codex_prompt_submit() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/root-cause-investigation why is usage not tracked?",
        "model": "gpt-5.5"
    });

    let input = first_event(raw, "codex");

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

    let input = first_event(raw, "codex");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_extracts_multiple_dollar_refs() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "prompt": "$root-cause-investigation and $security-review"
    });

    let batch = AttributionState::default().normalize(&raw, "codex");
    let mut names: Vec<_> = batch
        .occurrences
        .iter()
        .filter_map(|item| item.event.skill_name.clone())
        .collect();
    names.sort();

    assert_eq!(names, vec!["root-cause-investigation", "security-review"]);
}

#[test]
fn normalize_event_extracts_skill_from_claude_skill_tool_input() {
    let raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Skill",
        "tool_input": { "skill": "helper-gitlab" },
        "model": "haiku"
    });

    let input = first_event(raw, "claude");

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

    let input = first_event(raw, "codex");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(input.skill_name.as_deref(), Some("helper-gitlab"));
    assert_eq!(input.source_tool, "codex");
}

#[test]
fn normalize_event_extracts_multiple_dollar_prompt_refs() {
    let raw = json!({
        "event_type": "beforeSubmitPrompt",
        "prompt": "$root-cause-investigation and $security-review"
    });

    let batch = AttributionState::default().normalize(&raw, "cursor");
    assert_eq!(batch.occurrences.len(), 2);
}

#[test]
fn normalize_event_extracts_skill_from_cursor_skill_tool_input() {
    let raw = json!({
        "event_type": "postToolUse",
        "tool_name": "Skill",
        "tool_input": { "skill": "helper-gitlab" },
        "model": "composer-2.5"
    });

    let input = first_event(raw, "cursor");

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

    let input = first_event(raw, "cursor");

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

    let input = first_event(raw, "cursor");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
    assert_eq!(input.attribution_source.as_deref(), Some("slash_reference"));
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

    let input = first_event(raw, "cursor");

    assert_eq!(input.event_type, "PostToolUse");
    assert_eq!(
        input.skill_name.as_deref(),
        Some("root-cause-investigation")
    );
}

#[test]
fn normalize_event_extracts_multiple_attached_skills() {
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

    let batch = AttributionState::default().normalize(&raw, "cursor");
    let mut names: Vec<_> = batch
        .occurrences
        .iter()
        .filter_map(|item| item.event.skill_name.clone())
        .collect();
    names.sort();

    assert_eq!(names, vec!["helper-gitlab", "root-cause-investigation"]);
}

#[test]
fn normalize_extracts_spaced_slash_skill_from_codex_multi_skill_prompt() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "turn_id": "turn-grill",
        "prompt": "/root-cause-investigation /feature-dev /grill me about this plan"
    });

    let batch = AttributionState::default().normalize(&raw, "codex");
    let mut names: Vec<_> = batch
        .occurrences
        .iter()
        .filter_map(|item| item.event.skill_name.clone())
        .collect();
    names.sort();

    assert!(
        names.iter().any(|name| name == "grill-me"),
        "Codex must keep /grill me as grill-me, got {names:?}"
    );
    assert!(names.iter().any(|name| name == "root-cause-investigation"));
    assert!(names.iter().any(|name| name == "feature-dev"));
}

#[test]
fn normalize_extracts_spaced_slash_skill_from_claude_and_cursor() {
    let prompt = "/Repo Research what is this repo for?";
    for tool in ["claude", "cursor"] {
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "event_type": "beforeSubmitPrompt",
            "prompt": prompt
        });
        let batch = AttributionState::default().normalize(&raw, tool);
        let names: Vec<_> = batch
            .occurrences
            .iter()
            .filter_map(|item| item.event.skill_name.as_deref())
            .collect();
        assert!(
            names
                .iter()
                .any(|name| name.eq_ignore_ascii_case("repo-research")),
            "tool={tool} names={names:?}"
        );
    }
}

#[test]
fn normalize_reads_kiro_camel_case_prompt_submit() {
    let raw = json!({
        "hook_event_name": "userPromptSubmit",
        "session_id": "kiro-session",
        "prompt": "/repo-research what is this repo for?"
    });

    let input = first_event(raw, "kiro");

    assert_eq!(input.event_type, "PostSkillUse");
    assert_eq!(input.skill_name.as_deref(), Some("repo-research"));
    assert_eq!(input.source_tool, "kiro");
}
