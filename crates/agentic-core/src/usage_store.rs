//! Local SQLite store for opt-in skill/tool usage tracing.
//!
//! Hooks and collectors feed normalized events here; query methods return small
//! aggregates joined by Agentic Hub capability id. The store is local-only and
//! intentionally separate from remote Aptabase telemetry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::managed_copy::now_iso8601;
use crate::model::{
    CapabilityItem, CapabilityKind, CapabilityScope, UsageDashboard, UsageDashboardOverview,
    UsageDateRange, UsageDayBucket, UsageKindBucket, UsageSourceBucket, UsageStats,
    UsageToolBucket, UsageTopRow, UsageUnusedRow, UsageWorkspaceBucket,
};
use crate::paths::{home_dir, tildify};

mod queries;
mod read;
mod schema;

use queries::*;

const CURRENT_SCHEMA: u32 = 2;
const TERMINAL_EVENTS: [&str; 6] = [
    "PostToolUse",
    "PostToolUseFailure",
    "PostSkillUse",
    "PostAgentUse",
    "CommandPaletteUse",
    "McpToolComplete",
];
const METADATA_ALLOWLIST: [&str; 5] = ["branch", "model", "machine", "invocationType", "source"];

/// Canonical usage database path: `~/.agentic-hub/usage/trace.db`.
pub fn default_path() -> PathBuf {
    home_dir()
        .join(".agentic-hub")
        .join("usage")
        .join("trace.db")
}

/// Event payload accepted by the usage collector before resolution/persistence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEventInput {
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    pub source_tool: String,
    pub event_type: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub skill_name: Option<String>,
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub capability_scope: CapabilityScope,
    #[serde(default)]
    pub workspace_root: Option<String>,
    #[serde(default)]
    pub capability_relative_path: Option<String>,
    #[serde(default)]
    pub invocation_key: Option<String>,
    #[serde(default)]
    pub attribution_source: Option<String>,
    #[serde(default)]
    pub attribution_rank: u8,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub dedupe_hash: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// Scope-aware identity used to map persisted rows back to Manager row ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageCapabilityQuery {
    pub external_id: String,
    pub capability_id: String,
    pub capability_scope: CapabilityScope,
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageToolDiagnosticSummary {
    pub source_tool: String,
    pub last_captured_at: Option<String>,
    pub resolved_event_count: u32,
    pub unresolved_event_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedUsageReference {
    pub dedupe_hash: String,
    pub source_tool: String,
    pub skill_name: String,
    pub workspace_root: Option<String>,
}

impl UsageCapabilityQuery {
    pub fn from_item(item: &CapabilityItem) -> Self {
        let workspace = item.id.strip_prefix("ws::");
        Self {
            external_id: item.id.clone(),
            capability_id: workspace.unwrap_or(&item.id).to_string(),
            capability_scope: if workspace.is_some() || item.source_id == "workspace" {
                CapabilityScope::Workspace
            } else {
                CapabilityScope::Global
            },
            workspace_root: workspace.map(|_| item.source.rel_home.clone()),
        }
    }
}

/// SQLite-backed usage store.
#[derive(Debug, Clone)]
pub struct UsageStore {
    path: PathBuf,
}

impl Default for UsageStore {
    fn default() -> Self {
        UsageStore {
            path: default_path(),
        }
    }
}

impl UsageStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        UsageStore { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn connect(&self) -> Result<Connection> {
        schema::connect(&self.path)
    }

    /// Ensure the DB exists and migrations are applied.
    pub fn ensure_ready(&self) -> Result<()> {
        let _ = self.connect()?;
        Ok(())
    }

    /// Insert one normalized occurrence while preserving the legacy API.
    pub fn insert_event(&self, input: &UsageEventInput, items: &[CapabilityItem]) -> Result<()> {
        self.insert_events(std::slice::from_ref(input), items)
    }

    /// Transactionally insert a batch of normalized capability occurrences.
    ///
    /// A non-null invocation key identifies one capability within one user
    /// turn. A higher-confidence signal upgrades that row in place.
    pub fn insert_events(
        &self,
        inputs: &[UsageEventInput],
        items: &[CapabilityItem],
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let transaction = conn.transaction()?;
        for input in inputs.iter().filter(|input| has_attribution_signal(input)) {
            insert_event_row(&transaction, input, items)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Delete events that never resolved to a skill (`capability_id IS NULL`).
    pub fn purge_unresolved_events(&self) -> Result<u32> {
        let conn = self.connect()?;
        let removed = conn.execute("DELETE FROM usage_events WHERE capability_id IS NULL", [])?;
        Ok(removed as u32)
    }

    /// Delete unresolved rows that never carried a skill or capability reference.
    pub fn purge_unattributed_events(&self) -> Result<u32> {
        let conn = self.connect()?;
        let removed = conn.execute(
            r#"
            DELETE FROM usage_events
            WHERE capability_id IS NULL
              AND (skill_name IS NULL OR trim(skill_name) = '')
            "#,
            [],
        )?;
        Ok(removed as u32)
    }

    pub fn cleanup_before(&self, cutoff_iso: &str) -> Result<u32> {
        let conn = self.connect()?;
        let removed = conn.execute(
            "DELETE FROM usage_events WHERE timestamp < ?1",
            params![cutoff_iso],
        )?;
        Ok(removed as u32)
    }

    /// Record a command palette copy or paste against a scanned command id.
    pub fn record_palette_command_use(
        &self,
        items: &[CapabilityItem],
        capability_id: &str,
        pasted: bool,
    ) -> Result<()> {
        let input = UsageEventInput {
            source_tool: "agentic-hub".to_string(),
            event_type: "CommandPaletteUse".to_string(),
            capability_id: Some(capability_id.to_string()),
            success: Some(true),
            metadata: serde_json::json!({
                "invocationType": if pasted { "paste" } else { "copy" },
                "source": "palette",
            }),
            ..UsageEventInput::default()
        };
        self.insert_event(&input, items)
    }

    pub fn has_migration(&self, version: u32) -> Result<bool> {
        let conn = self.connect()?;
        let found: Option<u32> = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE version = ?1",
                params![version],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }
}

fn insert_event_row(
    conn: &Connection,
    input: &UsageEventInput,
    items: &[CapabilityItem],
) -> Result<()> {
    let event_id = trimmed(input.event_id.as_deref())
        .unwrap_or_else(|| format!("evt-{}", uuid::Uuid::new_v4()));
    let timestamp = trimmed(input.timestamp.as_deref()).unwrap_or_else(now_iso8601);
    let capability_id = resolve_capability_id(
        items,
        input.capability_id.as_deref(),
        input.skill_name.as_deref(),
    );
    let dedupe_hash = trimmed(input.dedupe_hash.as_deref())
        .unwrap_or_else(|| dedupe_hash(input, &event_id, &timestamp));
    let mut event_type = canonical_event_type(&input.event_type);
    if capability_id
        .as_deref()
        .is_some_and(|id| id.starts_with("agent:"))
        && event_type == "PostSkillUse"
    {
        event_type = "PostAgentUse".to_string();
    }
    let metadata_json = serde_json::to_string(&sanitize_metadata(&input.metadata))?;
    let success = success_value(&event_type, input.success);
    let workspace = input.workspace.as_deref().and_then(normalize_path_field);
    let workspace_root = input
        .workspace_root
        .as_deref()
        .and_then(normalize_path_field);
    let invocation_key = trimmed(input.invocation_key.as_deref());
    let attribution_source = trimmed(input.attribution_source.as_deref());

    conn.execute(
        r#"
        INSERT OR IGNORE INTO usage_events (
            event_id, timestamp, source_tool, event_type, tool_name,
            skill_name, capability_id, capability_scope, workspace_root,
            capability_relative_path, invocation_key, attribution_source,
            attribution_rank, workspace, project, success, duration_ms,
            dedupe_hash, metadata_json
        )
        VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
        )
        "#,
        params![
            event_id,
            timestamp,
            input.source_tool.trim(),
            event_type,
            trimmed(input.tool_name.as_deref()),
            trimmed(input.skill_name.as_deref()),
            capability_id,
            input.capability_scope.as_str(),
            workspace_root,
            trimmed(input.capability_relative_path.as_deref()),
            invocation_key,
            attribution_source,
            i64::from(input.attribution_rank),
            workspace,
            trimmed(input.project.as_deref()),
            if success { 1_i64 } else { 0_i64 },
            input.duration_ms.map(|value| value as i64),
            dedupe_hash,
            metadata_json,
        ],
    )?;

    if let Some(invocation_key) = trimmed(input.invocation_key.as_deref()) {
        conn.execute(
            r#"
            UPDATE usage_events
            SET event_type = ?1,
                timestamp = ?2,
                success = ?3,
                duration_ms = COALESCE(?4, duration_ms),
                attribution_source = ?5,
                attribution_rank = ?6
            WHERE invocation_key = ?7
              AND attribution_rank < ?6
            "#,
            params![
                event_type,
                timestamp,
                if success { 1_i64 } else { 0_i64 },
                input.duration_ms.map(|value| value as i64),
                attribution_source,
                i64::from(input.attribution_rank),
                invocation_key,
            ],
        )?;
    }
    Ok(())
}

/// SQL clause restricting `usage_events` to today's local calendar date,
/// independent of the dashboard's selected `UsageDateRange`. Stored timestamps
/// are UTC, so both sides are converted to the machine's local timezone —
/// otherwise the "today" boundary would flip at UTC midnight instead of the
/// user's actual midnight.
const TODAY_CLAUSE: &str = " AND date(timestamp, 'localtime') = date('now', 'localtime')";

fn has_attribution_signal(input: &UsageEventInput) -> bool {
    trimmed(input.capability_id.as_deref()).is_some()
        || trimmed(input.skill_name.as_deref()).is_some()
}

pub fn resolve_capability_id(
    items: &[CapabilityItem],
    capability_id: Option<&str>,
    skill_name: Option<&str>,
) -> Option<String> {
    if let Some(id) = trimmed(capability_id) {
        if capability_kind_from_id(&id).is_some() && items.iter().any(|item| item.id == id) {
            return Some(id);
        }
    }

    let name = trimmed(skill_name)?;
    let skill_match = resolve_for_kind(items, &name, CapabilityKind::Skill);
    let agent_match = resolve_for_kind(items, &name, CapabilityKind::Agent);
    match (skill_match, agent_match) {
        (Some(item), None) | (None, Some(item)) => Some(item.id.clone()),
        (Some(_), Some(_)) => None,
        (None, None) => None,
    }
}

fn resolve_for_kind<'a>(
    items: &'a [CapabilityItem],
    name: &str,
    kind: CapabilityKind,
) -> Option<&'a CapabilityItem> {
    let filtered: Vec<&CapabilityItem> = items.iter().filter(|item| item.kind == kind).collect();
    resolve_unique(filtered.iter().copied().filter(|item| item.name == name))
        .or_else(|| {
            resolve_unique(
                filtered
                    .iter()
                    .copied()
                    .filter(|item| item.relative_path.to_string_lossy().replace('\\', "/") == name),
            )
        })
        .or_else(|| {
            resolve_unique(filtered.iter().copied().filter(|item| {
                item.relative_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem == name)
            }))
        })
        .or_else(|| {
            let target = normalize_skill_name(name);
            resolve_unique(
                filtered
                    .iter()
                    .copied()
                    .filter(|item| normalize_skill_name(&item.name) == target),
            )
        })
}

fn resolve_unique<'a>(
    matches: impl Iterator<Item = &'a CapabilityItem>,
) -> Option<&'a CapabilityItem> {
    let mut iter = matches;
    let first = iter.next()?;
    if iter.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn normalize_skill_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn success_value(event_type: &str, success: Option<bool>) -> bool {
    success.unwrap_or(event_type != "PostToolUseFailure")
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

fn trimmed(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_path_field(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(tildify(Path::new(trimmed)))
    }
}

fn sanitize_metadata(value: &Value) -> Value {
    let mut out = Map::new();
    let Some(obj) = value.as_object() else {
        return Value::Object(out);
    };
    for key in METADATA_ALLOWLIST {
        if let Some(v) = obj.get(key) {
            if v.is_string() || v.is_number() || v.is_boolean() {
                out.insert(key.to_string(), v.clone());
            }
        }
    }
    Value::Object(out)
}

fn dedupe_hash(input: &UsageEventInput, event_id: &str, timestamp: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.source_tool.as_bytes());
    hasher.update(b"\0");
    hasher.update(input.event_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(event_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(timestamp.as_bytes());
    hasher.update(b"\0");
    if let Some(skill) = input.skill_name.as_deref() {
        hasher.update(skill.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Hash sensitive correlation material before it crosses the collector boundary.
pub fn hash_usage_correlation(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn terminal_sql_filter() -> String {
    TERMINAL_EVENTS
        .iter()
        .map(|event| format!("'{event}'"))
        .collect::<Vec<_>>()
        .join(",")
}

fn max_timestamp(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(a), Some(b)) => Some(if a >= b { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn usage_identity_key(
    scope: CapabilityScope,
    workspace_root: Option<&str>,
    capability_id: &str,
) -> String {
    format!(
        "{}\0{}\0{}",
        scope.as_str(),
        workspace_root.unwrap_or_default(),
        capability_id
    )
}

fn item_usage_identity_key(item: &CapabilityItem) -> String {
    let query = UsageCapabilityQuery::from_item(item);
    usage_identity_key(
        query.capability_scope,
        query.workspace_root.as_deref(),
        &query.capability_id,
    )
}

fn parse_scope(value: &str) -> CapabilityScope {
    if value == CapabilityScope::Workspace.as_str() {
        CapabilityScope::Workspace
    } else {
        CapabilityScope::Global
    }
}

fn capability_kind_from_id(capability_id: &str) -> Option<CapabilityKind> {
    if capability_id.starts_with("skill:") || capability_id.contains("::skill:") {
        Some(CapabilityKind::Skill)
    } else if capability_id.starts_with("agent:") || capability_id.contains("::agent:") {
        Some(CapabilityKind::Agent)
    } else if capability_id.starts_with("command:") || capability_id.contains("::command:") {
        Some(CapabilityKind::Command)
    } else {
        None
    }
}

fn capability_name_from_id(capability_id: &str) -> String {
    capability_id
        .rsplit([':', '/'])
        .next()
        .unwrap_or(capability_id)
        .trim_end_matches(".md")
        .to_string()
}

#[cfg(test)]
mod tests;
