#[cfg(test)]
mod legacy_normalization {
    use super::legacy_capability_refs::*;
    use super::*;

    pub(super) fn normalize_event(raw: Value, source_tool: &str) -> UsageEventInput {
        let raw_event_type = string_at(
            &raw,
            &[
                "hook_event_name",
                "hookEventName",
                "event_type",
                "eventType",
            ],
        )
        .unwrap_or_else(|| "unknown".to_string());
        let explicit_ref = if tool_supports_prompt_capability_attribution(source_tool)
            && raw_event_type.trim() != "UserPromptExpansion"
        {
            explicit_capability_reference(&raw)
        } else {
            None
        };
        let tool_name = string_at(&raw, &["tool_name", "toolName", "tool"]);
        let read_skill = skill_name_from_read_tool(&raw, tool_name.as_deref());
        let read_agent = agent_name_from_read_tool(&raw, tool_name.as_deref());
        let expansion_ref = capability_from_claude_prompt_expansion(&raw, source_tool);
        let event_type = if expansion_ref.is_some()
            || (prompt_submit_event(&raw_event_type) && explicit_ref.is_some())
        {
            "PostSkillUse".to_string()
        } else {
            canonical_event_type(&raw_event_type)
        };
        UsageEventInput {
            event_id: string_at(&raw, &["event_id", "eventId", "call_id", "tool_call_id"]),
            timestamp: string_at(&raw, &["timestamp"]).or_else(|| Some(now_iso8601())),
            source_tool: string_at(&raw, &["source_tool", "sourceTool"])
                .unwrap_or_else(|| source_tool.to_string()),
            event_type,
            tool_name,
            skill_name: string_at(&raw, &["skill_name", "skillName"])
                .or_else(|| {
                    nested_string_at(
                        &raw,
                        &["tool_input", "toolInput", "input"],
                        &["skill_name", "skillName", "skill", "name"],
                    )
                })
                .or(explicit_ref)
                .or(expansion_ref)
                .or(read_skill)
                .or(read_agent),
            capability_id: string_at(&raw, &["capability_id", "capabilityId"]),
            workspace: string_at(
                &raw,
                &["workspace", "workspace_path", "workspacePath", "cwd"],
            ),
            project: string_at(&raw, &["project", "repo", "repository"]),
            success: bool_at(&raw, &["success"]),
            duration_ms: u64_at(&raw, &["duration_ms", "durationMs"]),
            dedupe_hash: string_at(&raw, &["dedupe_hash", "dedupeHash"]),
            metadata: json!({
                "branch": string_at(&raw, &["branch"]),
                "model": string_at(&raw, &["model"]),
                "machine": string_at(&raw, &["machine"]),
                "invocationType": string_at(&raw, &["invocation_type", "invocationType"]),
                "source": string_at(&raw, &["source"]),
            }),
            ..UsageEventInput::default()
        }
    }

    fn canonical_event_type(event_type: &str) -> String {
        match event_type.trim() {
            "postToolUse" => "PostToolUse".to_string(),
            "postToolUseFailure" => "PostToolUseFailure".to_string(),
            "preToolUse" => "PreToolUse".to_string(),
            "beforeSubmitPrompt" => "UserPromptSubmit".to_string(),
            "stop" => "Stop".to_string(),
            other => other.to_string(),
        }
    }

    fn prompt_submit_event(event_type: &str) -> bool {
        matches!(event_type.trim(), "beforeSubmitPrompt" | "UserPromptSubmit")
    }

    fn capability_from_claude_prompt_expansion(raw: &Value, source_tool: &str) -> Option<String> {
        if source_tool != ToolId::Claude.as_str() {
            return None;
        }
        let event_type = string_at(
            raw,
            &[
                "hook_event_name",
                "hookEventName",
                "event_type",
                "eventType",
            ],
        )?;
        if event_type != "UserPromptExpansion" {
            return None;
        }
        let expansion_type = string_at(raw, &["expansion_type", "expansionType"])?;
        if expansion_type != "slash_command" {
            return None;
        }
        string_at(raw, &["command_name", "commandName"])
    }

    fn skill_name_from_read_tool(raw: &Value, tool_name: Option<&str>) -> Option<String> {
        if tool_name != Some("Read") {
            return None;
        }
        let path = nested_string_at(
            raw,
            &["tool_input", "toolInput", "input"],
            &["path", "file_path", "filePath", "target_file", "targetFile"],
        )?;
        skill_name_from_skill_md_path(&path)
    }

    fn skill_name_from_skill_md_path(path: &str) -> Option<String> {
        if !path.contains("SKILL.md") {
            return None;
        }
        let name = Path::new(path)
            .parent()
            .and_then(Path::file_name)
            .and_then(|segment| segment.to_str())?;
        let mut refs = BTreeSet::new();
        insert_skill_ref(name, &mut refs);
        refs.into_iter().next()
    }

    fn explicit_capability_reference(raw: &Value) -> Option<String> {
        explicit_skill_reference(raw)
    }

    fn explicit_skill_reference(raw: &Value) -> Option<String> {
        let mut refs = BTreeSet::new();
        collect_skill_refs(raw, &mut refs);
        collect_attachment_skill_refs(raw, &mut refs);
        if refs.len() == 1 {
            refs.into_iter().next()
        } else {
            None
        }
    }

    fn collect_attachment_skill_refs(raw: &Value, refs: &mut BTreeSet<String>) {
        let Some(attachments) = raw.get("attachments").and_then(Value::as_array) else {
            return;
        };
        for attachment in attachments {
            let Some(path) = attachment
                .get("file_path")
                .or_else(|| attachment.get("filePath"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let Some(name) = Path::new(path)
                .parent()
                .and_then(Path::file_name)
                .and_then(|segment| segment.to_str())
            else {
                continue;
            };
            if path.contains("SKILL.md") {
                insert_skill_ref(name, refs);
            }
        }
    }

    fn collect_skill_refs(value: &Value, refs: &mut BTreeSet<String>) {
        match value {
            Value::String(text) => collect_skill_refs_from_text(text, refs),
            Value::Array(values) => {
                for value in values {
                    collect_skill_refs(value, refs);
                }
            }
            Value::Object(map) => {
                for (key, value) in map {
                    if is_prompt_like_key(key) {
                        collect_skill_refs(value, refs);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_prompt_like_key(key: &str) -> bool {
        matches!(
            key,
            "prompt" | "userPrompt" | "message" | "text" | "input" | "content"
        )
    }

    fn collect_skill_refs_from_text(text: &str, refs: &mut BTreeSet<String>) {
        collect_markdown_skill_refs(text, refs);
        collect_dollar_skill_refs(text, refs);
        collect_slash_skill_refs(text, refs);
        collect_slash_capability_refs(text, refs);
        collect_at_agent_refs(text, refs);
        collect_skill_md_path_refs(text, refs);
        collect_agent_md_path_refs(text, refs);
    }

    fn collect_markdown_skill_refs(text: &str, refs: &mut BTreeSet<String>) {
        let mut rest = text;
        while let Some(start) = rest.find("[$") {
            let candidate = &rest[start + 2..];
            let Some(end) = candidate.find("](") else {
                break;
            };
            insert_skill_ref(&candidate[..end], refs);
            rest = &candidate[end + 2..];
        }
    }

    fn collect_dollar_skill_refs(text: &str, refs: &mut BTreeSet<String>) {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].1 != '$' {
                i += 1;
                continue;
            }
            let start = chars[i].0 + 1;
            let mut end = start;
            i += 1;
            while i < chars.len() {
                let ch = chars[i].1;
                if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/') {
                    end = chars[i].0 + ch.len_utf8();
                    i += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let token = &text[start..end];
                if token.contains('-') || token.contains('/') {
                    insert_skill_ref(token, refs);
                }
            }
        }
    }

    fn collect_slash_skill_refs(text: &str, refs: &mut BTreeSet<String>) {
        for token in text.split_whitespace() {
            let Some(rest) = token.strip_prefix('/') else {
                continue;
            };
            let cleaned = rest.trim_end_matches(|ch: char| {
                !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '/'
            });
            if cleaned.contains('-') || cleaned.contains('/') {
                insert_skill_ref(cleaned, refs);
            }
        }
    }

    fn collect_skill_md_path_refs(text: &str, refs: &mut BTreeSet<String>) {
        let mut rest = text;
        while let Some(end) = rest.find("/SKILL.md") {
            let before = &rest[..end];
            if let Some(name) = before.rsplit('/').next() {
                insert_skill_ref(name, refs);
            }
            rest = &rest[end + "/SKILL.md".len()..];
        }
    }

    fn insert_skill_ref(value: &str, refs: &mut BTreeSet<String>) {
        let cleaned = value.trim().trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '/'
        });
        if cleaned.is_empty() {
            return;
        }
        let leaf = cleaned.rsplit('/').next().unwrap_or(cleaned);
        if leaf.contains('-') {
            refs.insert(leaf.to_string());
        }
    }

    fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|key| {
            value
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
    }

    pub(super) fn nested_string_at(
        value: &Value,
        parents: &[&str],
        keys: &[&str],
    ) -> Option<String> {
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
}

#[cfg(test)]
mod legacy_capability_refs {
    use super::legacy_normalization::nested_string_at;
    use super::*;

    pub(super) fn collect_slash_capability_refs(text: &str, refs: &mut BTreeSet<String>) {
        for token in text.split_whitespace() {
            let Some(rest) = token.strip_prefix('/') else {
                continue;
            };
            let cleaned = rest
                .trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
            if cleaned.is_empty() || !is_capability_token(cleaned) {
                continue;
            }
            refs.insert(cleaned.to_string());
        }
    }

    pub(super) fn collect_at_agent_refs(text: &str, refs: &mut BTreeSet<String>) {
        for token in text.split_whitespace() {
            let Some(rest) = token.strip_prefix('@') else {
                continue;
            };
            let name = rest
                .strip_prefix("agent-")
                .unwrap_or(rest)
                .trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
            if name.is_empty() || !is_capability_token(name) {
                continue;
            }
            refs.insert(name.to_string());
        }
    }

    fn is_capability_token(token: &str) -> bool {
        token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    }

    pub(super) fn collect_agent_md_path_refs(text: &str, refs: &mut BTreeSet<String>) {
        let mut rest = text;
        while let Some(idx) = rest.find("/agents/") {
            let after = &rest[idx + "/agents/".len()..];
            let segment = after
                .split(['/', ' ', '\n', ')', ']'])
                .next()
                .unwrap_or(after);
            let stem = Path::new(segment)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(segment);
            if is_capability_token(stem) {
                refs.insert(stem.to_string());
            }
            rest = after;
        }
    }

    pub(super) fn agent_name_from_read_tool(
        raw: &Value,
        tool_name: Option<&str>,
    ) -> Option<String> {
        if tool_name != Some("Read") {
            return None;
        }
        let path = nested_string_at(
            raw,
            &["tool_input", "toolInput", "input"],
            &["path", "file_path", "filePath", "target_file", "targetFile"],
        )?;
        if !path.contains("/agents/") || !path.ends_with(".md") {
            return None;
        }
        Path::new(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| is_capability_token(stem))
            .map(str::to_string)
    }

    pub(super) fn tool_supports_prompt_capability_attribution(source_tool: &str) -> bool {
        tool_supports_prompt_skill_attribution(source_tool)
    }

    fn tool_supports_prompt_skill_attribution(source_tool: &str) -> bool {
        matches!(
            source_tool,
            tool if tool == ToolId::Cursor.as_str()
                || tool == ToolId::Codex.as_str()
                || tool == ToolId::Claude.as_str()
                || tool == ToolId::Kiro.as_str()
        )
    }
}

use super::*;
use legacy_normalization::normalize_event;

mod integration;
mod normalization;
