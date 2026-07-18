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
use crate::model::{
    CapabilityItem, CapabilityKind, UsageDashboard, UsageDashboardOverview, UsageDateRange,
    UsageDayBucket, UsageKindBucket, UsageSourceBucket, UsageStats, UsageToolBucket, UsageTopRow,
    UsageUnusedRow, UsageWorkspaceBucket,
};
use crate::paths::{home_dir, tildify};

const CURRENT_SCHEMA: u32 = 1;
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
    ///
    /// Events without an explicit `capability_id` or `skill_name` are ignored so
    /// generic tool calls do not accumulate as unresolved noise.
    pub fn insert_event(&self, input: &UsageEventInput, items: &[CapabilityItem]) -> Result<()> {
        if !has_attribution_signal(input) {
            return Ok(());
        }
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
        let mut event_type = canonical_event_type(&input.event_type);
        if let Some(id) = capability_id.as_deref() {
            if id.starts_with("agent:") && event_type == "PostSkillUse" {
                event_type = "PostAgentUse".to_string();
            }
        }
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

    /// Aggregate dashboard metrics for the Statistics page.
    pub fn query_dashboard(
        &self,
        items: &[CapabilityItem],
        range: UsageDateRange,
    ) -> Result<UsageDashboard> {
        let conn = self.connect()?;
        let terminal_filter = terminal_sql_filter();
        let range_clause = range_sql_clause(range);
        let countable: Vec<&CapabilityItem> =
            items.iter().filter(|item| is_countable(item)).collect();
        let item_map: HashMap<&str, &CapabilityItem> = countable
            .iter()
            .map(|item| (item.id.as_str(), *item))
            .collect();

        let total_events: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM usage_events WHERE 1=1 {range_clause}"),
            [],
            |row| row.get(0),
        )?;
        let terminal_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE event_type IN ({terminal_filter}) {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;
        let resolved_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NOT NULL {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;
        let unresolved_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NULL {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;

        let mut by_kind = query_kind_buckets(&conn, &terminal_filter, &range_clause)?;
        by_kind.retain(|bucket| bucket.kind != "other");
        let mut by_source_tool = query_source_buckets(&conn, &terminal_filter, &range_clause)?;
        let mut by_day = query_day_buckets(&conn, &terminal_filter, &range_clause)?;
        let mut by_workspace = query_workspace_buckets(&conn, &terminal_filter, &range_clause)?;

        let used_ids = query_used_capability_ids(&conn, &terminal_filter, &range_clause)?;
        let traced_capabilities = used_ids
            .iter()
            .filter(|id| item_map.contains_key(id.as_str()))
            .count() as u32;
        let unused_countable = countable
            .iter()
            .filter(|item| !used_ids.contains(&item.id))
            .count() as u32;

        let top_capabilities =
            query_top_capabilities(&conn, &terminal_filter, &range_clause, &item_map)?;
        let today_top_capabilities =
            query_top_capabilities(&conn, &terminal_filter, TODAY_CLAUSE, &item_map)?;
        let mut unused_capabilities: Vec<UsageUnusedRow> = countable
            .iter()
            .filter(|item| !used_ids.contains(&item.id))
            .map(|item| UsageUnusedRow {
                capability_id: item.id.clone(),
                name: item.name.clone(),
                kind: item.kind,
                source_label: item.source_label.clone(),
                relative_path: item.relative_path.to_string_lossy().into_owned(),
            })
            .collect();
        unused_capabilities.sort_by(|a, b| {
            a.kind
                .dir_name()
                .cmp(b.kind.dir_name())
                .then(a.name.cmp(&b.name))
        });

        sort_kind_buckets(&mut by_kind);
        by_source_tool.sort_by_key(|b| std::cmp::Reverse(b.execution_count));
        by_day.sort_by(|a, b| a.day.cmp(&b.day));
        by_workspace.sort_by_key(|b| std::cmp::Reverse(b.execution_count));

        Ok(UsageDashboard {
            overview: UsageDashboardOverview {
                total_events: total_events as u32,
                terminal_events: terminal_events as u32,
                resolved_events: resolved_events as u32,
                unresolved_events: unresolved_events as u32,
                traced_capabilities,
                installed_countable: countable.len() as u32,
                unused_countable,
            },
            by_kind,
            by_source_tool,
            by_day,
            top_capabilities,
            today_top_capabilities,
            unused_capabilities,
            by_workspace,
        })
    }
}

/// SQL clause restricting `usage_events` to today's local calendar date,
/// independent of the dashboard's selected `UsageDateRange`. Stored timestamps
/// are UTC, so both sides are converted to the machine's local timezone —
/// otherwise the "today" boundary would flip at UTC midnight instead of the
/// user's actual midnight.
const TODAY_CLAUSE: &str = " AND date(timestamp, 'localtime') = date('now', 'localtime')";

fn is_countable(item: &CapabilityItem) -> bool {
    matches!(
        item.kind,
        CapabilityKind::Skill | CapabilityKind::Command | CapabilityKind::Agent
    )
}

fn range_sql_clause(range: UsageDateRange) -> String {
    match range {
        UsageDateRange::Today => " AND timestamp >= datetime('now', '-1 days')".to_string(),
        UsageDateRange::Last7Days => " AND timestamp >= datetime('now', '-7 days')".to_string(),
        UsageDateRange::Last30Days => " AND timestamp >= datetime('now', '-30 days')".to_string(),
        UsageDateRange::Last90Days => " AND timestamp >= datetime('now', '-90 days')".to_string(),
        UsageDateRange::AllTime => String::new(),
    }
}

fn query_kind_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageKindBucket>> {
    let sql = format!(
        r#"
        SELECT CASE
                 WHEN capability_id LIKE 'skill:%' THEN 'skill'
                 WHEN capability_id LIKE 'command:%' THEN 'command'
                 WHEN capability_id LIKE 'agent:%' THEN 'agent'
                 ELSE 'other'
               END AS kind,
               COUNT(*) AS execution_count
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY kind
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageKindBucket {
            kind: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn sort_kind_buckets(buckets: &mut [UsageKindBucket]) {
    const ORDER: [&str; 3] = ["skill", "command", "agent"];
    buckets.sort_by(|a, b| {
        let ai = ORDER.iter().position(|k| *k == a.kind).unwrap_or(99);
        let bi = ORDER.iter().position(|k| *k == b.kind).unwrap_or(99);
        ai.cmp(&bi)
    });
}

fn query_source_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageSourceBucket>> {
    let sql = format!(
        r#"
        SELECT source_tool, COUNT(*) AS execution_count
        FROM usage_events
        WHERE event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY source_tool
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageSourceBucket {
            source_tool: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn query_day_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageDayBucket>> {
    let sql = format!(
        r#"
        SELECT date(timestamp) AS day, COUNT(*) AS execution_count
        FROM usage_events
        WHERE event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY day
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageDayBucket {
            day: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn query_workspace_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageWorkspaceBucket>> {
    let sql = format!(
        r#"
        SELECT workspace, COUNT(*) AS execution_count
        FROM usage_events
        WHERE workspace IS NOT NULL
          AND trim(workspace) != ''
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY workspace
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageWorkspaceBucket {
            workspace: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn query_used_capability_ids(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<HashSet<String>> {
    let sql = format!(
        r#"
        SELECT DISTINCT capability_id
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect())
}

fn query_top_capabilities(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
    item_map: &HashMap<&str, &CapabilityItem>,
) -> Result<Vec<UsageTopRow>> {
    let sql = format!(
        r#"
        SELECT capability_id,
               COUNT(*) AS execution_count,
               MAX(timestamp) AS last_used_at
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY capability_id
        ORDER BY execution_count DESC, capability_id ASC
        LIMIT 15
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let summary_rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)? as u32,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let bucket_sql = format!(
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
          {range_clause}
        GROUP BY capability_id, source_tool
        "#
    );
    let mut bucket_stmt = conn.prepare(&bucket_sql)?;
    let bucket_rows = bucket_stmt.query_map([], |row| {
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
    let mut buckets: HashMap<String, Vec<UsageToolBucket>> = HashMap::new();
    for row in bucket_rows {
        let (capability_id, bucket) = row?;
        buckets.entry(capability_id).or_default().push(bucket);
    }
    for list in buckets.values_mut() {
        list.sort_by(|a, b| a.source_tool.cmp(&b.source_tool));
    }

    let mut out = Vec::new();
    for row in summary_rows {
        let (capability_id, execution_count, last_used_at) = row?;
        let Some(item) = item_map.get(capability_id.as_str()) else {
            continue;
        };
        out.push(UsageTopRow {
            capability_id: capability_id.clone(),
            name: item.name.clone(),
            kind: item.kind,
            source_label: item.source_label.clone(),
            relative_path: item.relative_path.to_string_lossy().into_owned(),
            execution_count,
            last_used_at,
            tool_buckets: buckets.remove(&capability_id).unwrap_or_default(),
        });
    }
    Ok(out)
}

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
        if (id.starts_with("skill:") || id.starts_with("command:") || id.starts_with("agent:"))
            && items.iter().any(|item| item.id == id)
        {
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

    fn agent(id: &str, name: &str, relative_path: &str) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind: CapabilityKind::Agent,
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

    fn command(id: &str, name: &str, relative_path: &str) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind: CapabilityKind::Command,
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
    fn resolve_capability_id_matches_command_id() {
        let items = [command("command:git/commit.md", "commit", "git/commit.md")];

        let id = resolve_capability_id(&items, Some("command:git/commit.md"), None);

        assert_eq!(id.as_deref(), Some("command:git/commit.md"));
    }

    #[test]
    fn resolve_capability_id_matches_agent_name() {
        let items = [agent("agent:cto.md", "cto", "cto.md")];

        let id = resolve_capability_id(&items, None, Some("cto"));

        assert_eq!(id.as_deref(), Some("agent:cto.md"));
    }

    #[test]
    fn resolve_capability_id_returns_none_when_skill_and_agent_share_name() {
        let items = [
            skill("skill:cto", "cto", "cto"),
            agent("agent:cto.md", "cto", "cto.md"),
        ];

        let id = resolve_capability_id(&items, None, Some("cto"));

        assert_eq!(id, None);
    }

    #[test]
    fn insert_event_promotes_prompt_skill_use_to_post_agent_use() {
        let (_dir, store) = store();
        let items = [agent("agent:cto.md", "cto", "cto.md")];
        let mut input = event("cto", "agent-cto");
        input.event_type = "PostSkillUse".to_string();

        store.insert_event(&input, &items).unwrap();

        let conn = store.connect().unwrap();
        let event_type: String = conn
            .query_row(
                "SELECT event_type FROM usage_events WHERE dedupe_hash = 'agent-cto'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_type, "PostAgentUse");
    }

    #[test]
    fn record_palette_command_use_counts_in_query_stats() {
        let (_dir, store) = store();
        let items = [command("command:git/commit.md", "commit", "git/commit.md")];

        store
            .record_palette_command_use(&items, "command:git/commit.md", false)
            .unwrap();
        store
            .record_palette_command_use(&items, "command:git/commit.md", true)
            .unwrap();

        let stats = store
            .query_stats(&["command:git/commit.md".to_string()])
            .unwrap();

        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].execution_count, 2);
        assert_eq!(stats[0].tool_buckets[0].source_tool, "agentic-hub");
    }

    #[test]
    fn insert_event_skips_unattributed_post_tool_use() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        let input = UsageEventInput {
            source_tool: "cursor".to_string(),
            event_type: "PostToolUse".to_string(),
            tool_name: Some("Read".to_string()),
            dedupe_hash: Some("generic-read".to_string()),
            success: Some(true),
            ..UsageEventInput::default()
        };

        store.insert_event(&input, &items).unwrap();

        assert_eq!(store.event_count().unwrap(), 0);
    }

    #[test]
    fn purge_unattributed_events_keeps_ambiguous_skill_rows() {
        let (_dir, store) = store();
        let items = [skill("skill:tdd", "tdd", "tdd")];
        store
            .insert_event(&event("tdd", "resolved"), &items)
            .unwrap();
        store
            .insert_event(&event("missing", "ambiguous"), &items)
            .unwrap();
        let noise = UsageEventInput {
            source_tool: "cursor".to_string(),
            event_type: "PostToolUse".to_string(),
            tool_name: Some("Grep".to_string()),
            dedupe_hash: Some("noise".to_string()),
            success: Some(true),
            ..UsageEventInput::default()
        };
        store.insert_event(&noise, &items).unwrap();

        let removed = store.purge_unattributed_events().unwrap();

        assert_eq!(removed, 0);
        assert_eq!(store.event_count().unwrap(), 2);
        assert_eq!(store.resolved_event_count().unwrap(), 1);
        assert_eq!(store.unresolved_event_count().unwrap(), 1);
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
        store
            .insert_event(&event("tdd", "resolved"), &items)
            .unwrap();
        store
            .insert_event(&event("missing", "unresolved"), &items)
            .unwrap();

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

    #[test]
    fn query_dashboard_aggregates_terminal_usage_and_unused_rows() {
        let (_dir, store) = store();
        let items = [
            skill(
                "skill:root-cause-investigation",
                "root-cause-investigation",
                "root-cause-investigation",
            ),
            command("command:git/commit.md", "commit", "git/commit.md"),
            skill("skill:unused-skill", "unused-skill", "unused-skill"),
        ];
        let mut used = event("root-cause-investigation", "dash-a");
        used.event_type = "PostSkillUse".to_string();
        used.source_tool = "cursor".to_string();
        used.workspace = Some("~/Developer/demo".to_string());
        store.insert_event(&used, &items).unwrap();

        let palette = UsageEventInput {
            source_tool: "agentic-hub".to_string(),
            event_type: "CommandPaletteUse".to_string(),
            capability_id: Some("command:git/commit.md".to_string()),
            success: Some(true),
            dedupe_hash: Some("dash-palette".to_string()),
            timestamp: Some("2026-07-06T11:00:00Z".to_string()),
            ..UsageEventInput::default()
        };
        store.insert_event(&palette, &items).unwrap();

        let mut pre = event("unused-skill", "dash-pre");
        pre.event_type = "PreToolUse".to_string();
        store.insert_event(&pre, &items).unwrap();

        let dashboard = store
            .query_dashboard(&items, UsageDateRange::AllTime)
            .unwrap();

        assert_eq!(dashboard.overview.terminal_events, 2);
        assert_eq!(dashboard.overview.traced_capabilities, 2);
        assert_eq!(dashboard.overview.unused_countable, 1);
        assert_eq!(dashboard.by_kind.len(), 2);
        assert_eq!(dashboard.by_source_tool.len(), 2);
        assert_eq!(dashboard.top_capabilities.len(), 2);
        assert!(dashboard
            .top_capabilities
            .iter()
            .any(|row| row.capability_id == "skill:root-cause-investigation"));
        assert_eq!(dashboard.unused_capabilities.len(), 1);
        assert_eq!(
            dashboard.unused_capabilities[0].capability_id,
            "skill:unused-skill"
        );
        assert_eq!(dashboard.by_workspace.len(), 1);
        assert_eq!(dashboard.by_workspace[0].workspace, "~/Developer/demo");
    }

    #[test]
    fn query_dashboard_today_top_capabilities_excludes_older_events() {
        let (_dir, store) = store();
        let items = [
            skill("skill:root-cause-investigation", "root-cause-investigation", "root-cause-investigation"),
            skill("skill:tdd", "tdd", "tdd"),
        ];

        let mut today = event("root-cause-investigation", "today-event");
        today.timestamp = Some(crate::managed_copy::now_iso8601());
        store.insert_event(&today, &items).unwrap();

        let mut old = event("tdd", "old-event");
        old.timestamp = Some("2020-01-01T00:00:00Z".to_string());
        store.insert_event(&old, &items).unwrap();

        let dashboard = store
            .query_dashboard(&items, UsageDateRange::AllTime)
            .unwrap();

        assert_eq!(dashboard.top_capabilities.len(), 2, "both events count toward all-time top usage");
        assert_eq!(
            dashboard.today_top_capabilities.len(),
            1,
            "only the event timestamped today should appear"
        );
        assert_eq!(
            dashboard.today_top_capabilities[0].capability_id,
            "skill:root-cause-investigation"
        );
    }
}
