//! Local SQLite store for opt-in skill/tool usage tracing.
//!
//! Hooks and collectors feed normalized events here; query methods return small
//! aggregates joined by Agentic Hub capability id. The store is local-only and
//! intentionally separate from remote Aptabase telemetry.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::managed_copy::now_iso8601;
use crate::model::{CapabilityItem, CapabilityKind, UsageStats, UsageToolBucket};
use crate::paths::{home_dir, tildify};

const CURRENT_SCHEMA: u32 = 1;
const TERMINAL_EVENTS: [&str; 4] = [
    "PostToolUse",
    "PostToolUseFailure",
    "PostSkillUse",
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

    fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        self.migrate(&conn)?;
        Ok(conn)
    }

    fn migrate(&self, conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS usage_events (
                event_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                source_tool TEXT NOT NULL,
                event_type TEXT NOT NULL,
                tool_name TEXT,
                skill_name TEXT,
                capability_id TEXT,
                workspace TEXT,
                project TEXT,
                success INTEGER NOT NULL,
                duration_ms INTEGER,
                dedupe_hash TEXT NOT NULL UNIQUE,
                metadata_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_usage_events_capability
                ON usage_events(capability_id, event_type, timestamp);
            CREATE INDEX IF NOT EXISTS idx_usage_events_source_tool
                ON usage_events(source_tool, timestamp);
            "#,
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            params![CURRENT_SCHEMA, now_iso8601()],
        )?;
        Ok(())
    }

    /// Ensure the DB exists and migrations are applied.
    pub fn ensure_ready(&self) -> Result<()> {
        let _ = self.connect()?;
        Ok(())
    }

    /// Insert a normalized event. Duplicate `dedupe_hash` values are ignored.
    pub fn insert_event(&self, input: &UsageEventInput, items: &[CapabilityItem]) -> Result<()> {
        let conn = self.connect()?;
        let event_id = input
            .event_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("evt-{}", uuid::Uuid::new_v4()));
        let timestamp = input
            .timestamp
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(now_iso8601);
        let capability_id = resolve_capability_id(
            items,
            input.capability_id.as_deref(),
            input.skill_name.as_deref(),
        );
        let dedupe_hash = input
            .dedupe_hash
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| dedupe_hash(input, &event_id, &timestamp));
        let event_type = canonical_event_type(&input.event_type);
        let metadata_json = serde_json::to_string(&sanitize_metadata(&input.metadata))?;
        let success = success_value(&event_type, input.success);
        let workspace = input.workspace.as_deref().and_then(normalize_path_field);

        conn.execute(
            r#"
            INSERT OR IGNORE INTO usage_events (
                event_id, timestamp, source_tool, event_type, tool_name,
                skill_name, capability_id, workspace, project, success,
                duration_ms, dedupe_hash, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                event_id,
                timestamp,
                input.source_tool.trim(),
                event_type,
                trimmed(input.tool_name.as_deref()),
                trimmed(input.skill_name.as_deref()),
                capability_id,
                workspace,
                trimmed(input.project.as_deref()),
                if success { 1_i64 } else { 0_i64 },
                input.duration_ms.map(|v| v as i64),
                dedupe_hash,
                metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Query terminal usage counts for the provided capability ids.
    pub fn query_stats(&self, capability_ids: &[String]) -> Result<Vec<UsageStats>> {
        if capability_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.connect()?;
        let live_ids: HashSet<&str> = capability_ids.iter().map(String::as_str).collect();
        let mut stats: HashMap<String, UsageStats> = capability_ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    UsageStats {
                        capability_id: id.clone(),
                        ..UsageStats::default()
                    },
                )
            })
            .collect();

        let terminal_filter = terminal_sql_filter();
        let mut stmt = conn.prepare(&format!(
            r#"
            SELECT capability_id,
                   source_tool,
                   COUNT(*) AS execution_count,
                   SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) AS success_count,
                   SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END) AS failure_count,
                   MAX(timestamp) AS last_used_at
            FROM usage_events
            WHERE capability_id IS NOT NULL
              AND event_type IN ({terminal_filter})
            GROUP BY capability_id, source_tool
            "#
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                UsageToolBucket {
                    source_tool: row.get(1)?,
                    execution_count: row.get::<_, i64>(2)? as u32,
                    success_count: row.get::<_, i64>(3)? as u32,
                    failure_count: row.get::<_, i64>(4)? as u32,
                    last_used_at: row.get(5)?,
                },
            ))
        })?;

        for row in rows {
            let (capability_id, bucket) = row?;
            if !live_ids.contains(capability_id.as_str()) {
                continue;
            }
            if let Some(stat) = stats.get_mut(&capability_id) {
                stat.execution_count += bucket.execution_count;
                stat.success_count += bucket.success_count;
                stat.failure_count += bucket.failure_count;
                stat.last_used_at =
                    max_timestamp(stat.last_used_at.take(), bucket.last_used_at.clone());
                stat.tool_buckets.push(bucket);
            }
        }

        let mut out: Vec<UsageStats> = stats
            .into_values()
            .filter(|s| s.execution_count > 0)
            .collect();
        out.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));
        for stat in &mut out {
            stat.tool_buckets
                .sort_by(|a, b| a.source_tool.cmp(&b.source_tool));
        }
        Ok(out)
    }

    pub fn event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
        Ok(count as u32)
    }

    pub fn resolved_event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn unresolved_event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Re-run skill resolution for stored rows that have a `skill_name` but no
    /// `capability_id`. Returns the number of rows updated.
    pub fn re_resolve_events(&self, items: &[CapabilityItem]) -> Result<u32> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT dedupe_hash, skill_name
            FROM usage_events
            WHERE capability_id IS NULL
              AND skill_name IS NOT NULL
              AND trim(skill_name) != ''
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0_u32;
        for row in rows {
            let (dedupe_hash, skill_name) = row?;
            let Some(capability_id) = resolve_capability_id(items, None, Some(&skill_name)) else {
                continue;
            };
            let changed = conn.execute(
                "UPDATE usage_events SET capability_id = ?1 WHERE dedupe_hash = ?2 AND capability_id IS NULL",
                params![capability_id, dedupe_hash],
            )?;
            updated += changed as u32;
        }
        Ok(updated)
    }

    /// Delete events that never resolved to a skill (`capability_id IS NULL`).
    pub fn purge_unresolved_events(&self) -> Result<u32> {
        let conn = self.connect()?;
        let removed = conn.execute(
            "DELETE FROM usage_events WHERE capability_id IS NULL",
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

pub fn resolve_capability_id(
    items: &[CapabilityItem],
    capability_id: Option<&str>,
    skill_name: Option<&str>,
) -> Option<String> {
    let skills: Vec<&CapabilityItem> = items
        .iter()
        .filter(|item| item.kind == CapabilityKind::Skill)
        .collect();

    if let Some(id) = trimmed(capability_id) {
        if id.starts_with("skill:") && skills.iter().any(|item| item.id == id) {
            return Some(id);
        }
    }

    let name = trimmed(skill_name)?;
    resolve_unique(skills.iter().copied().filter(|item| item.name == name))
        .or_else(|| {
            resolve_unique(
                skills
                    .iter()
                    .copied()
                    .filter(|item| item.relative_path.to_string_lossy().replace('\\', "/") == name),
            )
        })
        .or_else(|| {
            let target = normalize_skill_name(&name);
            resolve_unique(
                skills
                    .iter()
                    .copied()
                    .filter(|item| normalize_skill_name(&item.name) == target),
            )
        })
        .map(|item| item.id.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceRef;

    fn store() -> (tempfile::TempDir, UsageStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::with_path(dir.path().join("trace.db"));
        (dir, store)
    }

    fn skill(id: &str, name: &str, relative_path: &str) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind: CapabilityKind::Skill,
            name: name.to_string(),
            source_path: PathBuf::from(format!("/src/{relative_path}")),
            relative_path: PathBuf::from(relative_path),
            source_id: "default".to_string(),
            source_label: "Default".to_string(),
            source: SourceRef {
                rel_home: "~/.agentic".to_string(),
                folder: ".agentic".to_string(),
            },
            valid: true,
            validation_errors: Vec::new(),
        }
    }

    fn event(skill_name: &str, hash: &str) -> UsageEventInput {
        UsageEventInput {
            event_id: Some(hash.to_string()),
            timestamp: Some("2026-07-06T10:00:00Z".to_string()),
            source_tool: "claude".to_string(),
            event_type: "PostToolUse".to_string(),
            skill_name: Some(skill_name.to_string()),
            dedupe_hash: Some(hash.to_string()),
            success: Some(true),
            ..UsageEventInput::default()
        }
    }

    #[test]
    fn ensure_ready_creates_schema_migration() {
        let (_dir, store) = store();

        store.ensure_ready().unwrap();

        assert!(store.has_migration(CURRENT_SCHEMA).unwrap());
    }

    #[test]
    fn insert_event_ignores_duplicate_dedupe_hash() {
        let (_dir, store) = store();
        let items = [skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        )];
        let input = event("root-cause-investigation", "same");

        store.insert_event(&input, &items).unwrap();
        store.insert_event(&input, &items).unwrap();

        assert_eq!(store.event_count().unwrap(), 1);
    }

    #[test]
    fn query_stats_counts_terminal_events_by_tool() {
        let (_dir, store) = store();
        let items = [skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        )];
        store
            .insert_event(&event("root-cause-investigation", "a"), &items)
            .unwrap();
        let mut second = event("root-cause-investigation", "b");
        second.source_tool = "codex".to_string();
        second.success = Some(false);
        store.insert_event(&second, &items).unwrap();

        let stats = store
            .query_stats(&["skill:root-cause-investigation".to_string()])
            .unwrap();

        assert_eq!(
            stats,
            vec![UsageStats {
                capability_id: "skill:root-cause-investigation".to_string(),
                execution_count: 2,
                success_count: 1,
                failure_count: 1,
                last_used_at: Some("2026-07-06T10:00:00Z".to_string()),
                tool_buckets: vec![
                    UsageToolBucket {
                        source_tool: "claude".to_string(),
                        execution_count: 1,
                        success_count: 1,
                        failure_count: 0,
                        last_used_at: Some("2026-07-06T10:00:00Z".to_string()),
                    },
                    UsageToolBucket {
                        source_tool: "codex".to_string(),
                        execution_count: 1,
                        success_count: 0,
                        failure_count: 1,
                        last_used_at: Some("2026-07-06T10:00:00Z".to_string()),
                    },
                ],
            }]
        );
    }

    #[test]
    fn query_stats_counts_cursor_lower_camel_terminal_events() {
        let (_dir, store) = store();
        let items = [skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        )];
        let mut input = event("root-cause-investigation", "cursor-a");
        input.source_tool = "cursor".to_string();
        input.event_type = "postToolUse".to_string();

        store.insert_event(&input, &items).unwrap();

        let stats = store
            .query_stats(&["skill:root-cause-investigation".to_string()])
            .unwrap();

        assert_eq!(stats[0].execution_count, 1);
        assert_eq!(stats[0].tool_buckets[0].source_tool, "cursor");
        assert_eq!(stats[0].tool_buckets[0].execution_count, 1);
    }

    #[test]
    fn query_stats_excludes_non_terminal_events() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        let mut input = event("tdd", "pre");
        input.event_type = "PreToolUse".to_string();

        store.insert_event(&input, &items).unwrap();

        assert!(store
            .query_stats(&["skill:tdd".to_string()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn resolve_capability_id_matches_exact_id() {
        let items = [skill("skill:dev/tdd", "tdd", "dev/tdd")];

        let id = resolve_capability_id(&items, Some("skill:dev/tdd"), Some("other"));

        assert_eq!(id.as_deref(), Some("skill:dev/tdd"));
    }

    #[test]
    fn resolve_capability_id_matches_relative_path() {
        let items = [skill("skill:dev/tdd", "tdd", "dev/tdd")];

        let id = resolve_capability_id(&items, None, Some("dev/tdd"));

        assert_eq!(id.as_deref(), Some("skill:dev/tdd"));
    }

    #[test]
    fn resolve_capability_id_matches_normalized_slug() {
        let items = [skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        )];

        let id = resolve_capability_id(&items, None, Some("Root Cause Investigation"));

        assert_eq!(id.as_deref(), Some("skill:root-cause-investigation"));
    }

    #[test]
    fn resolve_capability_id_returns_none_when_name_is_ambiguous() {
        let items = [
            skill("skill:a/root-cause", "root-cause", "a/root-cause"),
            skill("skill:b/root-cause", "root-cause", "b/root-cause"),
        ];

        let id = resolve_capability_id(&items, None, Some("root-cause"));

        assert_eq!(id, None);
    }

    #[test]
    fn unresolved_events_are_stored_without_visible_stats() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];

        store
            .insert_event(&event("missing", "missing"), &items)
            .unwrap();

        assert_eq!(store.event_count().unwrap(), 1);
        assert!(store
            .query_stats(&["skill:tdd".to_string()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn re_resolve_events_updates_rows_with_skill_name() {
        let (_dir, store) = store();
        let items = [skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        )];
        store
            .insert_event(&event("root-cause-investigation", "re-resolve"), &[])
            .unwrap();

        let updated = store.re_resolve_events(&items).unwrap();

        assert_eq!(updated, 1);
        assert_eq!(store.resolved_event_count().unwrap(), 1);
        assert_eq!(store.unresolved_event_count().unwrap(), 0);
    }

    #[test]
    fn purge_unresolved_events_removes_only_unresolved_rows() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        store.insert_event(&event("tdd", "resolved"), &items).unwrap();
        store.insert_event(&event("missing", "unresolved"), &items).unwrap();

        let removed = store.purge_unresolved_events().unwrap();

        assert_eq!(removed, 1);
        assert_eq!(store.event_count().unwrap(), 1);
        assert_eq!(store.resolved_event_count().unwrap(), 1);
        assert_eq!(store.unresolved_event_count().unwrap(), 0);
    }

    #[test]
    #[ignore = "manual: purges unresolved rows from local ~/.agentic-hub/usage/trace.db"]
    fn purge_live_unresolved_usage_db_manual() {
        let store = UsageStore::default();
        let before = store.event_count().unwrap();
        let unresolved = store.unresolved_event_count().unwrap();
        let removed = store.purge_unresolved_events().unwrap();
        let after = store.event_count().unwrap();
        eprintln!("purged {removed} unresolved rows ({before} -> {after} total)");
        assert_eq!(removed, unresolved);
    }

    #[test]
    #[ignore = "manual: re-resolves the local ~/.agentic-hub/usage/trace.db"]
    fn re_resolve_live_usage_db_manual() {
        use crate::api;
        use crate::settings::Settings;

        let store = UsageStore::default();
        let settings = Settings::load().expect("settings");
        let scan = api::scan(&settings);
        let before = store.unresolved_event_count().unwrap();
        let updated = store.re_resolve_events(&scan.items).unwrap();
        let after = store.unresolved_event_count().unwrap();
        eprintln!("re-resolve updated {updated} rows ({before} -> {after} unresolved)");
    }

    #[test]
    fn cleanup_before_removes_older_events() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        let mut old = event("tdd", "old");
        old.timestamp = Some("2026-01-01T00:00:00Z".to_string());
        let mut new = event("tdd", "new");
        new.timestamp = Some("2026-07-06T00:00:00Z".to_string());
        store.insert_event(&old, &items).unwrap();
        store.insert_event(&new, &items).unwrap();

        let removed = store.cleanup_before("2026-07-01T00:00:00Z").unwrap();

        assert_eq!(removed, 1);
        assert_eq!(store.event_count().unwrap(), 1);
    }

    #[test]
    fn insert_event_redacts_metadata_to_allowlist() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        let mut input = event("tdd", "redact");
        input.metadata = serde_json::json!({
            "branch": "main",
            "prompt": "secret prompt",
            "arguments": { "code": "secret code" }
        });
        store.insert_event(&input, &items).unwrap();
        let conn = store.connect().unwrap();

        let metadata: String = conn
            .query_row("SELECT metadata_json FROM usage_events", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(metadata, r#"{"branch":"main"}"#);
    }
}
