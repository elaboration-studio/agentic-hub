use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::Result;
use crate::managed_copy::now_iso8601;

use super::CURRENT_SCHEMA;

pub(super) fn connect(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
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
            capability_scope TEXT NOT NULL DEFAULT 'global',
            workspace_root TEXT,
            capability_relative_path TEXT,
            invocation_key TEXT,
            attribution_source TEXT,
            attribution_rank INTEGER NOT NULL DEFAULT 0,
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
    add_column_if_missing(conn, "capability_scope", "TEXT NOT NULL DEFAULT 'global'")?;
    add_column_if_missing(conn, "workspace_root", "TEXT")?;
    add_column_if_missing(conn, "capability_relative_path", "TEXT")?;
    add_column_if_missing(conn, "invocation_key", "TEXT")?;
    add_column_if_missing(conn, "attribution_source", "TEXT")?;
    add_column_if_missing(conn, "attribution_rank", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_events_invocation_key
            ON usage_events(invocation_key) WHERE invocation_key IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_usage_events_scoped_capability
            ON usage_events(capability_scope, workspace_root, capability_id, timestamp);
        "#,
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        params![CURRENT_SCHEMA, now_iso8601()],
    )?;
    Ok(())
}

fn add_column_if_missing(conn: &Connection, name: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(usage_events)")?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if names.iter().any(|existing| existing == name) {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE usage_events ADD COLUMN {name} {definition}"
    ))?;
    Ok(())
}
