//! Privacy-safe normalization of hook payloads into explicit skill references.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agentic_core::managed_copy::now_iso8601;
use agentic_core::{hash_usage_correlation, UsageEventInput};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOccurrence {
    pub event: UsageEventInput,
    pub exact_skill_path: Option<PathBuf>,
    pub requires_catalog_match: bool,
    pub correlation_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NormalizedBatch {
    pub occurrences: Vec<NormalizedOccurrence>,
    pub workspace_roots: Vec<PathBuf>,
}

#[derive(Default)]
pub struct AttributionState {
    session_turns: Mutex<HashMap<String, u64>>,
}

#[derive(Debug, Clone)]
struct SkillReference {
    name: String,
    exact_path: Option<PathBuf>,
    source: &'static str,
    rank: u8,
    requires_catalog_match: bool,
}

impl AttributionState {
    pub fn normalize(&self, raw: &Value, source_tool: &str) -> NormalizedBatch {
        let raw_event_type = string_at(
            raw,
            &[
                "hook_event_name",
                "hookEventName",
                "event_type",
                "eventType",
            ],
        )
        .unwrap_or_else(|| "unknown".to_string());
        let event_type = canonical_event_type(&raw_event_type);
        let correlation_hash = self.correlation_hash(raw, source_tool, &event_type);
        let references = extract_references(raw, source_tool, &event_type);
        let workspace_roots = workspace_roots(raw);
        let source_tool = string_at(raw, &["source_tool", "sourceTool"])
            .unwrap_or_else(|| source_tool.to_string());
        let tool_name = string_at(raw, &["tool_name", "toolName", "tool"]);
        let timestamp = string_at(raw, &["timestamp"]).or_else(|| Some(now_iso8601()));

        let occurrences = references
            .into_iter()
            .map(|reference| NormalizedOccurrence {
                event: UsageEventInput {
                    event_id: string_at(raw, &["event_id", "eventId", "call_id", "tool_call_id"]),
                    timestamp: timestamp.clone(),
                    source_tool: source_tool.clone(),
                    event_type: if prompt_event(&event_type) {
                        "PostSkillUse".to_string()
                    } else {
                        event_type.clone()
                    },
                    tool_name: tool_name.clone(),
                    skill_name: Some(reference.name),
                    workspace: workspace_roots
                        .first()
                        .map(|path| path.to_string_lossy().into_owned()),
                    project: string_at(raw, &["project", "repo", "repository"]),
                    success: bool_at(raw, &["success"]),
                    duration_ms: u64_at(raw, &["duration_ms", "durationMs"]),
                    attribution_source: Some(reference.source.to_string()),
                    attribution_rank: reference.rank,
                    metadata: json!({
                        "branch": string_at(raw, &["branch"]),
                        "model": string_at(raw, &["model"]),
                        "machine": string_at(raw, &["machine"]),
                        "invocationType": string_at(raw, &["invocation_type", "invocationType"]),
                        "source": string_at(raw, &["source"]),
                    }),
                    ..UsageEventInput::default()
                },
                exact_skill_path: reference.exact_path,
                requires_catalog_match: reference.requires_catalog_match,
                correlation_hash: correlation_hash.clone(),
            })
            .collect();
        NormalizedBatch {
            occurrences,
            workspace_roots,
        }
    }

    fn correlation_hash(&self, raw: &Value, source_tool: &str, event_type: &str) -> String {
        let direct = match source_tool {
            "cursor" => string_at(raw, &["generation_id", "generationId"]),
            "codex" => string_at(raw, &["turn_id", "turnId"]),
            "claude" | "grok" => string_at(raw, &["prompt_id", "promptId"]),
            "kiro" => string_at(raw, &["turn_id", "turnId"]),
            _ => None,
        };
        if let Some(value) = direct {
            return hash_usage_correlation(&format!("{source_tool}\0{value}"));
        }
        if matches!(source_tool, "claude" | "kiro") {
            if let Some(session) = string_at(raw, &["session_id", "sessionId"]) {
                let mut turns = match self.session_turns.lock() {
                    Ok(turns) => turns,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let session_hash = hash_usage_correlation(&session);
                let key = format!("{source_tool}\0{session_hash}");
                let turn = turns.entry(key).or_default();
                if event_type == "UserPromptSubmit" {
                    *turn = turn.saturating_add(1);
                }
                return hash_usage_correlation(&format!("{source_tool}\0{session_hash}\0{turn}"));
            }
        }
        let fallback = string_at(raw, &["event_id", "eventId", "call_id", "tool_call_id"])
            .or_else(|| string_at(raw, &["timestamp"]))
            .unwrap_or_else(now_iso8601);
        hash_usage_correlation(&format!("{source_tool}\0{fallback}"))
    }
}

fn extract_references(raw: &Value, source_tool: &str, event_type: &str) -> Vec<SkillReference> {
    let mut refs = Vec::new();
    let tool_name = string_at(raw, &["tool_name", "toolName", "tool"]);
    if tool_name.as_deref() == Some("Skill") {
        if let Some(name) = nested_string_at(
            raw,
            &["tool_input", "toolInput", "input"],
            &["skill_name", "skillName", "skill", "name"],
        ) {
            push_name_ref(&mut refs, &name, "skill_tool", 100, false);
        }
    }
    if let Some(name) = string_at(raw, &["skill_name", "skillName"]) {
        push_name_ref(&mut refs, &name, "explicit_field", 100, false);
    }
    if source_tool == "claude"
        && event_type == "UserPromptExpansion"
        && string_at(raw, &["expansion_type", "expansionType"]).as_deref() == Some("slash_command")
    {
        if let Some(name) = string_at(raw, &["command_name", "commandName"]) {
            push_name_ref(&mut refs, &name, "prompt_expansion", 90, false);
        }
    }
    if matches!(tool_name.as_deref(), Some("Read" | "read_file")) {
        if let Some(path) = nested_string_at(
            raw,
            &["tool_input", "toolInput", "input"],
            &["path", "file_path", "filePath", "target_file", "targetFile"],
        ) {
            push_path_ref(&mut refs, &path, "skill_read", "agent_read", 100);
        }
    }
    if supports_prompt_attribution(source_tool) && event_type == "UserPromptSubmit" {
        for key in ["prompt", "user_prompt", "userPrompt", "message"] {
            if let Some(text) = raw.get(key).and_then(Value::as_str) {
                collect_prompt_refs(text, &mut refs);
            }
        }
        collect_attachment_refs(raw, &mut refs);
    }
    dedupe_references(refs)
}

fn collect_prompt_refs(text: &str, refs: &mut Vec<SkillReference>) {
    let mut rest = text;
    while let Some(start) = rest.find("[$") {
        let after = &rest[start + 2..];
        let Some(label_end) = after.find("](") else {
            break;
        };
        let after_link = &after[label_end + 2..];
        let Some(path_end) = after_link.find(')') else {
            break;
        };
        push_path_ref(
            refs,
            &after_link[..path_end],
            "skill_link",
            "agent_link",
            70,
        );
        rest = &after_link[path_end + 1..];
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (index, token) in tokens.iter().copied().enumerate() {
        if let Some(name) = token.strip_prefix('$') {
            push_joined_name_refs(
                refs,
                name,
                &tokens[index + 1..],
                "dollar_reference",
                50,
                false,
            );
        } else if let Some(name) = token.strip_prefix('/') {
            push_joined_name_refs(
                refs,
                strip_qualified_slash(name),
                &tokens[index + 1..],
                "slash_reference",
                60,
                true,
            );
        } else if let Some(rest) = token.strip_prefix('@') {
            let name = rest.strip_prefix("agent-").unwrap_or(rest);
            if let Some(name) = clean_token(Some(name)) {
                push_name_ref(refs, name, "agent_mention", 60, true);
            }
        }
    }
}

fn push_joined_name_refs(
    refs: &mut Vec<SkillReference>,
    first: &str,
    following: &[&str],
    source: &'static str,
    rank: u8,
    requires_catalog_match: bool,
) {
    let Some(first) = clean_token(Some(first)) else {
        return;
    };
    let mut parts = vec![first.to_string()];
    push_name_ref(refs, &parts.join("-"), source, rank, requires_catalog_match);
    if first.contains('-') || first.contains('_') {
        return;
    }
    for extra in following.iter().copied().take(1) {
        if extra.starts_with('/') || extra.starts_with('$') || extra.starts_with('@') {
            break;
        }
        let Some(next) = clean_token(Some(extra)) else {
            break;
        };
        if is_join_stopword(next) {
            break;
        }
        parts.push(next.to_string());
        push_name_ref(refs, &parts.join("-"), source, rank, requires_catalog_match);
    }
}

fn collect_attachment_refs(raw: &Value, refs: &mut Vec<SkillReference>) {
    let Some(attachments) = raw.get("attachments").and_then(Value::as_array) else {
        return;
    };
    for attachment in attachments {
        if let Some(path) = string_at(attachment, &["file_path", "filePath", "path"]) {
            push_path_ref(refs, &path, "skill_attachment", "agent_attachment", 70);
        }
    }
}

fn push_path_ref(
    refs: &mut Vec<SkillReference>,
    raw_path: &str,
    skill_source: &'static str,
    agent_source: &'static str,
    rank: u8,
) {
    let path = PathBuf::from(raw_path);
    if path_is_skill_file(&path) {
        let Some(name) = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
        else {
            return;
        };
        if valid_skill_name(name) {
            refs.push(SkillReference {
                name: name.to_string(),
                exact_path: Some(path),
                source: skill_source,
                rank,
                requires_catalog_match: true,
            });
        }
        return;
    }
    if let Some(name) = agent_name_from_path(&path) {
        refs.push(SkillReference {
            name,
            exact_path: Some(path),
            source: agent_source,
            rank,
            requires_catalog_match: true,
        });
    }
}

fn push_name_ref(
    refs: &mut Vec<SkillReference>,
    name: &str,
    source: &'static str,
    rank: u8,
    requires_catalog_match: bool,
) {
    let name = name.trim().trim_start_matches('/').trim_start_matches('$');
    let leaf = name.rsplit('/').next().unwrap_or(name);
    if valid_skill_name(leaf) {
        refs.push(SkillReference {
            name: leaf.to_string(),
            exact_path: None,
            source,
            rank,
            requires_catalog_match,
        });
    }
}

fn dedupe_references(refs: Vec<SkillReference>) -> Vec<SkillReference> {
    let mut out: BTreeMap<String, SkillReference> = BTreeMap::new();
    for reference in refs {
        let key = reference.exact_path.as_ref().map_or_else(
            || reference.name.clone(),
            |path| path.to_string_lossy().into_owned(),
        );
        let replace = out
            .get(&key)
            .map_or(true, |existing| existing.rank < reference.rank);
        if replace {
            out.insert(key, reference);
        }
    }
    out.into_values().collect()
}

fn workspace_roots(raw: &Value) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(values) = raw
        .get("workspace_roots")
        .or_else(|| raw.get("workspaceRoots"))
        .and_then(Value::as_array)
    {
        roots.extend(values.iter().filter_map(Value::as_str).map(PathBuf::from));
    }
    if roots.is_empty() {
        if let Some(path) = string_at(
            raw,
            &["cwd", "workspace", "workspace_path", "workspacePath"],
        ) {
            roots.push(PathBuf::from(path));
        }
    }
    roots
}

fn clean_token(value: Option<&str>) -> Option<&str> {
    let token = value?.trim_end_matches(|ch: char| {
        !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '/'
    });
    (!token.is_empty() && !token.contains('.') && !token.contains(".."))
        .then_some(token.rsplit('/').next().unwrap_or(token))
}

fn is_join_stopword(name: &str) -> bool {
    matches!(
        name,
        "and"
            | "or"
            | "the"
            | "a"
            | "an"
            | "to"
            | "for"
            | "of"
            | "in"
            | "on"
            | "with"
            | "as"
            | "at"
            | "by"
            | "from"
    )
}

fn strip_qualified_slash(name: &str) -> &str {
    let Some((prefix, rest)) = name.split_once(':') else {
        return name;
    };
    if prefix.is_empty()
        || rest.is_empty()
        || !prefix
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return name;
    }
    rest
}

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn supports_prompt_attribution(tool: &str) -> bool {
    is_traced_source_tool(tool)
}

pub fn is_traced_source_tool(tool: &str) -> bool {
    matches!(tool, "cursor" | "claude" | "codex" | "kiro" | "grok")
}

fn prompt_event(event_type: &str) -> bool {
    matches!(event_type, "UserPromptSubmit" | "UserPromptExpansion")
}

fn canonical_event_type(event_type: &str) -> String {
    match event_type.trim() {
        "postToolUse" => "PostToolUse",
        "postToolUseFailure" => "PostToolUseFailure",
        "preToolUse" => "PreToolUse",
        "beforeSubmitPrompt" | "userPromptSubmit" | "promptSubmit" | "user_prompt_submit" => {
            "UserPromptSubmit"
        }
        "post_tool_use" => "PostToolUse",
        "post_tool_use_failure" => "PostToolUseFailure",
        "pre_tool_use" => "PreToolUse",
        other => other,
    }
    .to_string()
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn nested_string_at(value: &Value, parents: &[&str], keys: &[&str]) -> Option<String> {
    parents.iter().find_map(|parent| {
        value
            .get(*parent)
            .and_then(|nested| string_at(nested, keys))
    })
}

fn bool_at(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn u64_at(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

pub fn path_is_skill_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
}

pub fn path_is_capability_file(path: &Path) -> bool {
    path_is_skill_file(path) || agent_name_from_path(path).is_some()
}

fn agent_name_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if !file_name.ends_with(".md") || file_name == "SKILL.md" {
        return None;
    }
    let has_agents = path
        .components()
        .any(|component| component.as_os_str() == "agents");
    if !has_agents {
        return None;
    }
    let stem = file_name.trim_end_matches(".md");
    valid_skill_name(stem).then(|| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_returns_every_distinct_skill_reference() {
        let state = AttributionState::default();
        let raw = json!({
            "event_type": "beforeSubmitPrompt",
            "generation_id": "generation-1",
            "prompt": "$feature-dev and $root-cause-investigation and $feature-dev"
        });

        let batch = state.normalize(&raw, "cursor");

        assert_eq!(
            batch
                .occurrences
                .iter()
                .filter_map(|item| item.event.skill_name.as_deref())
                .collect::<Vec<_>>(),
            vec!["feature-dev", "root-cause-investigation"]
        );
    }

    #[test]
    fn normalize_extracts_slash_skill_from_cursor_prompt_submit() {
        let state = AttributionState::default();
        let raw = json!({
            "event_type": "beforeSubmitPrompt",
            "generation_id": "generation-1",
            "prompt": "/root-cause-investigation why is usage not tracked?"
        });

        let batch = state.normalize(&raw, "cursor");

        assert_eq!(batch.occurrences.len(), 1);
        assert_eq!(
            batch.occurrences[0].event.skill_name.as_deref(),
            Some("root-cause-investigation")
        );
        assert_eq!(
            batch.occurrences[0].event.attribution_source.as_deref(),
            Some("slash_reference")
        );
        assert!(batch.occurrences[0].requires_catalog_match);
        assert_eq!(batch.occurrences[0].event.attribution_rank, 60);
    }

    #[test]
    fn normalize_extracts_slash_skill_from_codex_and_claude_prompt_submit() {
        let state = AttributionState::default();
        let prompt = "/root-cause-investigation why is usage not tracked?";
        for tool in ["codex", "claude"] {
            let raw = json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt": prompt
            });
            let batch = state.normalize(&raw, tool);
            assert_eq!(
                batch.occurrences[0].event.skill_name.as_deref(),
                Some("root-cause-investigation"),
                "tool={tool}"
            );
            assert!(batch.occurrences[0].requires_catalog_match);
        }
    }

    #[test]
    fn normalize_extracts_slash_agent_and_at_mention() {
        let state = AttributionState::default();
        let slash = state.normalize(
            &json!({
                "event_type": "beforeSubmitPrompt",
                "prompt": "/cto investigate this bug"
            }),
            "cursor",
        );
        assert_eq!(
            slash.occurrences[0].event.skill_name.as_deref(),
            Some("cto")
        );
        assert_eq!(
            slash.occurrences[0].event.attribution_source.as_deref(),
            Some("slash_reference")
        );

        let mention = state.normalize(
            &json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt": "@agent-cto investigate this bug"
            }),
            "claude",
        );
        assert_eq!(
            mention.occurrences[0].event.skill_name.as_deref(),
            Some("cto")
        );
        assert_eq!(
            mention.occurrences[0].event.attribution_source.as_deref(),
            Some("agent_mention")
        );
    }

    #[test]
    fn normalize_extracts_agent_from_read_tool_path() {
        let state = AttributionState::default();
        let raw = json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/Users/me/.cursor/agents/cto.md"
            }
        });

        let batch = state.normalize(&raw, "cursor");

        assert_eq!(
            batch.occurrences[0].event.skill_name.as_deref(),
            Some("cto")
        );
        assert_eq!(
            batch.occurrences[0].event.attribution_source.as_deref(),
            Some("agent_read")
        );
        assert!(batch.occurrences[0].requires_catalog_match);
    }

    #[test]
    fn normalize_keeps_slash_health_as_catalog_required_candidate() {
        let state = AttributionState::default();
        let raw = json!({
            "event_type": "beforeSubmitPrompt",
            "prompt": "/health /tmp/pasted-text.txt /tmp/clipboard-image.png"
        });

        let batch = state.normalize(&raw, "cursor");

        assert_eq!(batch.occurrences.len(), 1);
        assert_eq!(
            batch.occurrences[0].event.skill_name.as_deref(),
            Some("health")
        );
        assert!(batch.occurrences[0].requires_catalog_match);
    }

    #[test]
    fn normalize_returns_distinct_slash_dollar_and_tool_skills() {
        let state = AttributionState::default();
        let raw = json!({
            "event_type": "beforeSubmitPrompt",
            "generation_id": "generation-1",
            "prompt": "/alpha-skill and $beta-skill"
        });
        let tool = json!({
            "event_type": "postToolUse",
            "generation_id": "generation-1",
            "tool_name": "Skill",
            "tool_input": { "skill": "gamma-skill" }
        });

        let prompt_batch = state.normalize(&raw, "cursor");
        let tool_batch = state.normalize(&tool, "cursor");

        let mut names: Vec<_> = prompt_batch
            .occurrences
            .iter()
            .chain(tool_batch.occurrences.iter())
            .filter_map(|item| item.event.skill_name.clone())
            .collect();
        names.sort();
        assert_eq!(names, vec!["alpha-skill", "beta-skill", "gamma-skill"]);
    }

    #[test]
    fn normalize_ignores_health_images_and_pasted_files() {
        let state = AttributionState::default();
        let raw = json!({
            "event_type": "beforeSubmitPrompt",
            "prompt": "/tmp/pasted-text.txt /tmp/clipboard-image.png"
        });

        let batch = state.normalize(&raw, "cursor");

        assert!(batch.occurrences.is_empty());
    }

    #[test]
    fn claude_session_tracker_correlates_expansion_with_prompt_turn() {
        let state = AttributionState::default();
        let prompt = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "private-session",
            "prompt": "$feature-dev"
        });
        let expansion = json!({
            "hook_event_name": "UserPromptExpansion",
            "session_id": "private-session",
            "expansion_type": "slash_command",
            "command_name": "feature-dev"
        });

        let prompt_batch = state.normalize(&prompt, "claude");
        let expansion_batch = state.normalize(&expansion, "claude");

        assert_eq!(
            prompt_batch.occurrences[0].correlation_hash,
            expansion_batch.occurrences[0].correlation_hash
        );
    }

    #[test]
    fn normalized_event_never_contains_raw_prompt_or_tool_input() {
        let state = AttributionState::default();
        let raw = json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Skill",
            "tool_input": { "skill": "feature-dev", "secret": "do-not-store" },
            "prompt": "do-not-store"
        });

        let batch = state.normalize(&raw, "claude");
        let serialized = serde_json::to_string(&batch.occurrences[0].event).unwrap();

        assert!(!serialized.contains("do-not-store"));
    }
}
