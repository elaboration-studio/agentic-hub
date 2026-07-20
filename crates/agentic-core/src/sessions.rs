//! Read-only local session history for Codex, Claude Code, and Cursor.
//!
//! Every reader opens its source read-only and derives metadata + a title;
//! nothing here writes into a tool's own store. There is no persisted index
//! yet, so listing re-reads every source on each call — the metadata-only
//! SQLite index (with mtime-based skip) is a follow-up slice, not a
//! requirement for this one.
//!
//! Claude and Codex are one JSONL file per session. Cursor is different: every
//! session ("composer") and every message ("bubble") lives as its own row in
//! one shared SQLite KV store (`cursorDiskKV` in `globalStorage/state.vscdb`),
//! and a single machine can accumulate tens of thousands of bubble rows across
//! hundreds of MB. Reading every bubble's content on every list load is not
//! viable (measured ~8s / ~470MB for one real profile), so the Cursor reader
//! only reads bubble *keys* (cheap: `bubbleId:<composerId>:<bubbleId>`, grouped
//! in SQL) plus the single earliest/latest bubble *value* per composer for
//! title/workspace/timestamp — full per-session content is read on demand in
//! `read_cursor_transcript`, consistent with the on-demand-content design.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::model::{ToolId, UsageDateRange};
use crate::paths::{home_dir, tildify};

/// One role a transcript message can carry.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    User,
    Assistant,
    Tool,
    System,
}

/// Normalized session metadata, one per Codex/Claude/Cursor session. Contains
/// no transcript text — only titles and identifiers meant for local display.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// Stable id: `"<tool>:<native session id>"`.
    pub session_key: String,
    pub tool: ToolId,
    pub title: String,
    /// Tildified `cwd`/folder, or `None` when the source recorded none.
    pub workspace: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: u32,
    /// Absolute path to the session's source file (Claude/Codex), or a
    /// store-specific locator when the source isn't a standalone file — for
    /// Cursor this is the composer id within the shared `state.vscdb`.
    pub source_path: String,
}

/// One transcript message, read on demand from the source file.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: SessionRole,
    pub text: String,
    pub tool_name: Option<String>,
    pub timestamp: Option<String>,
}

/// Filter applied to an in-memory session list.
#[derive(Debug, Clone, Default)]
pub struct SessionListFilter {
    pub tools: Option<Vec<ToolId>>,
    pub workspace: Option<String>,
    pub query: Option<String>,
}

/// Root directory Claude Code writes session JSONL under.
pub fn claude_projects_root() -> PathBuf {
    home_dir().join(".claude").join("projects")
}

/// Root directory Codex writes rollout JSONL under.
pub fn codex_sessions_root() -> PathBuf {
    home_dir().join(".codex").join("sessions")
}

/// Codex's own id-to-title index, refreshed by the Codex CLI itself.
pub fn codex_session_index_path() -> PathBuf {
    home_dir().join(".codex").join("session_index.jsonl")
}

/// List every local Claude Code session, newest activity first not guaranteed
/// (callers should sort the merged set). Missing/unreadable source data is
/// skipped rather than surfaced as an error, since a tool that was never run
/// (or whose store moved) is a normal, empty state.
///
/// `range` is applied *before* reading a file's content: a session's JSONL
/// file is append-only, so its mtime tracks last activity, and a plain
/// `fs::metadata` stat is orders of magnitude cheaper than parsing the file.
/// This is what makes a narrow default range (e.g. last 7 days) actually save
/// time rather than just trimming an already-fully-read list.
pub fn list_claude_sessions(range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    list_claude_sessions_at(&claude_projects_root(), range)
}

fn list_claude_sessions_at(root: &Path, range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    let cutoff = range_cutoff_system_time(range);
    let Ok(project_dirs) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for project_dir in project_dirs.flatten() {
        let Ok(file_type) = project_dir.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(project_dir.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if !mtime_within_range(&path, cutoff) {
                continue;
            }
            if let Some(summary) = summarize_claude_session(&path) {
                out.push(summary);
            }
        }
    }
    Ok(out)
}

/// List every local Codex session across the day-sharded `sessions/` tree,
/// joined against `session_index.jsonl` for its curated `thread_name` titles.
/// See [`list_claude_sessions`] for why `range` is applied via a cheap mtime
/// stat before any file content is read.
pub fn list_codex_sessions(range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    list_codex_sessions_at(&codex_sessions_root(), range)
}

fn list_codex_sessions_at(root: &Path, range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    let cutoff = range_cutoff_system_time(range);
    let mut files = Vec::new();
    collect_jsonl_files(root, 4, &mut files);
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let titles = read_codex_session_index();
    let mut out = Vec::new();
    for path in files {
        if !mtime_within_range(&path, cutoff) {
            continue;
        }
        if let Some(summary) = summarize_codex_session(&path, &titles) {
            out.push(summary);
        }
    }
    Ok(out)
}

/// Merge every supported tool's sessions, newest activity first. Sessions
/// without a resolvable timestamp sort last. `range` narrows and speeds up
/// every reader — see their individual docs for how each applies it cheaply.
pub fn list_all_sessions(range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    let mut sessions = list_claude_sessions(range)?;
    sessions.extend(list_codex_sessions(range)?);
    sessions.extend(list_cursor_sessions(range)?);
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// The `SystemTime` cutoff for `range`, or `None` for `AllTime` (no filter).
fn range_cutoff_system_time(range: UsageDateRange) -> Option<std::time::SystemTime> {
    let days = range_days(range)?;
    Some(std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400))
}

/// The same cutoff as [`range_cutoff_system_time`], as an ISO 8601 string, for
/// comparison against Cursor's string-typed bubble timestamps.
fn range_cutoff_iso8601(range: UsageDateRange) -> Option<String> {
    let days = range_days(range)?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(epoch_millis_to_iso8601(now_ms - (days as i64) * 86_400_000))
}

fn range_days(range: UsageDateRange) -> Option<u64> {
    match range {
        UsageDateRange::Today => Some(1),
        UsageDateRange::Last7Days => Some(7),
        UsageDateRange::Last30Days => Some(30),
        UsageDateRange::Last90Days => Some(90),
        UsageDateRange::AllTime => None,
    }
}

/// `true` when `path`'s mtime is at or after `cutoff` (or `cutoff` is `None`,
/// i.e. `AllTime`). A file whose mtime can't be read is kept rather than
/// silently dropped — the reader that actually opens it is the right place to
/// give up on an unreadable source.
fn mtime_within_range(path: &Path, cutoff: Option<std::time::SystemTime>) -> bool {
    let Some(cutoff) = cutoff else {
        return true;
    };
    match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(mtime) => mtime >= cutoff,
        Err(_) => true,
    }
}

/// Apply tool/workspace/query filters to an already-loaded session list. A
/// query under two characters matches everything (mirrors the skills search
/// convention of not firing on a bare keystroke).
pub fn filter_sessions(sessions: Vec<SessionSummary>, filter: &SessionListFilter) -> Vec<SessionSummary> {
    let query = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| q.chars().count() >= 2)
        .map(str::to_lowercase);

    sessions
        .into_iter()
        .filter(|s| {
            if let Some(tools) = &filter.tools {
                if !tools.contains(&s.tool) {
                    return false;
                }
            }
            if let Some(workspace) = &filter.workspace {
                if s.workspace.as_deref() != Some(workspace.as_str()) {
                    return false;
                }
            }
            if let Some(q) = &query {
                let haystack = format!(
                    "{} {} {}",
                    s.title,
                    s.workspace.as_deref().unwrap_or(""),
                    s.git_branch.as_deref().unwrap_or("")
                )
                .to_lowercase();
                if !haystack.contains(q.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Read one session's full transcript live from its source. `tool` selects
/// the parser; nothing is written back to any store. Claude/Codex resolve
/// `source_path` as a file; Cursor resolves it as a composer id within the
/// shared `state.vscdb` (see the module doc for why).
pub fn read_session_transcript(tool: ToolId, source_path: &str) -> Result<Vec<SessionMessage>> {
    match tool {
        ToolId::Claude | ToolId::Codex => {
            let path = Path::new(source_path);
            if !path.is_file() {
                return Err(CoreError::SessionSourceUnavailable(source_path.to_string()));
            }
            match tool {
                ToolId::Claude => read_claude_transcript(path),
                ToolId::Codex => read_codex_transcript(path),
                _ => unreachable!(),
            }
        }
        ToolId::Cursor => read_cursor_transcript(source_path),
        _ => Err(CoreError::SessionSourceUnavailable(format!(
            "no session reader for tool {:?}",
            tool
        ))),
    }
}

// ---- Claude Code ---------------------------------------------------------

fn summarize_claude_session(path: &Path) -> Option<SessionSummary> {
    let Ok(content) = fs::read_to_string(path) else {
        return None;
    };
    let session_id = path.file_stem()?.to_str()?.to_string();

    let mut workspace: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut title: Option<String> = None;
    let mut message_count: u32 = 0;
    let mut fallback_slug: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if workspace.is_none() {
            if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
                workspace = Some(tildify(Path::new(cwd)));
            }
        }
        if git_branch.is_none() {
            if let Some(branch) = value.get("gitBranch").and_then(Value::as_str) {
                git_branch = Some(branch.to_string());
            }
        }
        if fallback_slug.is_none() {
            if let Some(slug) = value.get("slug").and_then(Value::as_str) {
                fallback_slug = Some(slug.to_string());
            }
        }
        if let Some(ts) = value.get("timestamp").and_then(Value::as_str) {
            if started_at.is_none() {
                started_at = Some(ts.to_string());
            }
            updated_at = Some(ts.to_string());
        }

        let line_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        if line_type == "user" || line_type == "assistant" {
            message_count += 1;
        }
        if title.is_none() && line_type == "user" {
            if let Some(text) = claude_message_text(&value) {
                title = claude_title_from_text(&text);
            }
        }
    }

    let title = title
        .or(fallback_slug)
        .unwrap_or_else(|| "Untitled session".to_string());

    Some(SessionSummary {
        session_key: format!("claude:{session_id}"),
        tool: ToolId::Claude,
        title,
        workspace,
        git_branch,
        model: None,
        started_at,
        updated_at,
        message_count,
        source_path: path.to_string_lossy().into_owned(),
    })
}

/// The first text-bearing string out of a Claude `user`/`assistant` line's
/// `message.content` — a plain string, or the first `type: "text"` block.
fn claude_message_text(line: &Value) -> Option<String> {
    let content = line.get("message")?.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let blocks = content.as_array()?;
    blocks
        .iter()
        .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|b| b.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Slash-command payloads wrap the real ask in `<command-args>`; prefer that,
/// otherwise fall back to the first non-empty line, truncated for list display.
fn claude_title_from_text(text: &str) -> Option<String> {
    let candidate = extract_tag(text, "command-args").unwrap_or_else(|| text.to_string());
    first_line_title(&candidate)
}

fn extract_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

/// The first non-empty line of `text`, trimmed and truncated for list display.
fn first_line_title(text: &str) -> Option<String> {
    let first_line = text.lines().find(|l| !l.trim().is_empty())?.trim();
    if first_line.is_empty() {
        None
    } else {
        Some(truncate_title(first_line))
    }
}

fn truncate_title(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(MAX_CHARS).collect();
    format!("{truncated}…")
}

fn read_claude_transcript(path: &Path) -> Result<Vec<SessionMessage>> {
    let content = fs::read_to_string(path)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let line_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        let role = match line_type {
            "user" => SessionRole::User,
            "assistant" => SessionRole::Assistant,
            _ => continue,
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(content) = value.get("message").and_then(|m| m.get("content")) else {
            continue;
        };
        push_claude_content_messages(content, role, timestamp.as_deref(), &mut messages);
    }
    Ok(messages)
}

fn push_claude_content_messages(
    content: &Value,
    role: SessionRole,
    timestamp: Option<&str>,
    out: &mut Vec<SessionMessage>,
) {
    if let Some(text) = content.as_str() {
        if !text.trim().is_empty() {
            out.push(SessionMessage {
                role,
                text: text.to_string(),
                tool_name: None,
                timestamp: timestamp.map(str::to_string),
            });
        }
        return;
    }
    let Some(blocks) = content.as_array() else {
        return;
    };
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        out.push(SessionMessage {
                            role,
                            text: text.to_string(),
                            tool_name: None,
                            timestamp: timestamp.map(str::to_string),
                        });
                    }
                }
            }
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string();
                let input = block
                    .get("input")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                out.push(SessionMessage {
                    role: SessionRole::Tool,
                    text: format!("{name}({input})"),
                    tool_name: Some(name),
                    timestamp: timestamp.map(str::to_string),
                });
            }
            _ => {}
        }
    }
}

// ---- Codex ----------------------------------------------------------------

fn read_codex_session_index() -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Ok(content) = fs::read_to_string(codex_session_index_path()) else {
        return out;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let (Some(id), Some(name)) = (
            value.get("id").and_then(Value::as_str),
            value.get("thread_name").and_then(Value::as_str),
        ) else {
            continue;
        };
        out.insert(id.to_string(), name.to_string());
    }
    out
}

fn summarize_codex_session(
    path: &Path,
    titles: &std::collections::HashMap<String, String>,
) -> Option<SessionSummary> {
    let Ok(content) = fs::read_to_string(path) else {
        return None;
    };
    let mut lines = content.lines();
    let meta_line = lines.find(|l| !l.trim().is_empty())?;
    let meta: Value = serde_json::from_str(meta_line).ok()?;
    if meta.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = meta.get("payload")?;
    let id = payload.get("id").and_then(Value::as_str)?.to_string();
    let started_at = payload
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let workspace = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(|cwd| tildify(Path::new(cwd)));
    let model = payload
        .get("model_provider")
        .and_then(Value::as_str)
        .map(str::to_string);
    let git_branch = payload
        .get("git")
        .and_then(|g| g.get("branch"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut message_count: u32 = 0;
    let mut updated_at = started_at.clone();
    let mut first_user_message: Option<String> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(ts) = value.get("timestamp").and_then(Value::as_str) {
            updated_at = Some(ts.to_string());
        }
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("user_message") | Some("agent_message") => {
                message_count += 1;
                if first_user_message.is_none() {
                    if let Some(text) = payload.get("message").and_then(Value::as_str) {
                        first_user_message = Some(truncate_title(text));
                    }
                }
            }
            _ => {}
        }
    }

    let title = titles
        .get(&id)
        .cloned()
        .or(first_user_message)
        .unwrap_or_else(|| "Untitled session".to_string());

    Some(SessionSummary {
        session_key: format!("codex:{id}"),
        tool: ToolId::Codex,
        title,
        workspace,
        git_branch,
        model,
        started_at,
        updated_at,
        message_count,
        source_path: path.to_string_lossy().into_owned(),
    })
}

fn read_codex_transcript(path: &Path) -> Result<Vec<SessionMessage>> {
    let content = fs::read_to_string(path)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let role = match payload.get("type").and_then(Value::as_str) {
            Some("user_message") => SessionRole::User,
            Some("agent_message") => SessionRole::Assistant,
            _ => continue,
        };
        let Some(text) = payload.get("message").and_then(Value::as_str) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        messages.push(SessionMessage {
            role,
            text: text.to_string(),
            tool_name: None,
            timestamp,
        });
    }
    Ok(messages)
}

// ---- Cursor -----------------------------------------------------------------
//
// Cursor keeps every session ("composer") and message ("bubble") as its own
// row in one shared `cursorDiskKV` SQLite KV table, keyed `composerData:<id>`
// and `bubbleId:<composerId>:<bubbleId>`. `value` is declared `BLOB` but
// Cursor writes UTF-8 JSON as either storage class depending on version, so
// every read here tolerates both. Bubble rows carry their own ISO 8601
// `createdAt` and a `workspaceUris` array (`file://` URI), so per-message
// timestamps and workspace attribution come straight from the bubble — no
// join against `workspaceStorage/*/workspace.json` is needed. `type: 1` is a
// user bubble, `type: 2` is an assistant bubble (confirmed against real
// content, not documented anywhere by Cursor).

/// Root directory holding Cursor's Electron app-support data. macOS is the
/// exercised target; Windows/Linux paths follow Electron's usual convention
/// but are untested (mirrors the honesty note on `paths::home_dir`).
fn cursor_app_support_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home_dir())
            .join("Cursor")
    } else if cfg!(target_os = "macos") {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Cursor")
    } else {
        home_dir().join(".config").join("Cursor")
    }
}

/// The shared SQLite KV store every Cursor composer and bubble lives in.
pub fn cursor_global_db_path() -> PathBuf {
    cursor_app_support_root()
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// Open Cursor's KV store read-only. SQLite's WAL mode lets a read-only
/// reader see a consistent snapshot without contending with a running
/// Cursor's writes, so no copy is needed.
fn open_cursor_db_readonly(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

/// Read a `cursorDiskKV.value` column as parsed JSON, accepting either the
/// `Text` or `Blob` SQLite storage class (see the section doc above).
fn cursor_json_column(row: &Row<'_>, idx: usize) -> Option<Value> {
    let bytes: Vec<u8> = match row.get_ref(idx).ok()? {
        ValueRef::Text(b) => b.to_vec(),
        ValueRef::Blob(b) => b.to_vec(),
        _ => return None,
    };
    serde_json::from_slice(&bytes).ok()
}

/// A `bubbleId:<composerId>:<bubbleId>` key's composer id starts right after
/// the 9-char `"bubbleId:"` prefix (SQL `substr` is 1-indexed, so position 10)
/// and is a 36-char UUID. Shared by every query below that groups bubbles by
/// composer without reading `value`.
const CURSOR_BUBBLE_COMPOSER_ID_SQL: &str = "substr(key, 10, 36)";

/// Bubble count per composer id. Reads only `key` text (fast: ~0.03s over
/// 67k rows on a real profile), never `value`, so this stays cheap regardless
/// of how much transcript content a machine has accumulated.
fn cursor_bubble_counts(conn: &Connection) -> std::collections::HashMap<String, u32> {
    let mut out = std::collections::HashMap::new();
    let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT {CURSOR_BUBBLE_COMPOSER_ID_SQL} AS cid, COUNT(*) FROM cursorDiskKV WHERE key LIKE 'bubbleId:%' GROUP BY cid",
    )) else {
        return out;
    };
    let Ok(rows) =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)))
    else {
        return out;
    };
    for (cid, count) in rows.flatten() {
        out.insert(cid, count);
    }
    out
}

/// The earliest (or latest) bubble's parsed value per composer id, keyed by
/// `rowid` — a` MIN`/`MAX` aggregate grouped in SQL, so only ~1 small row per
/// composer is fetched instead of every bubble. Used for title, workspace,
/// and start/end timestamps without reading full transcript content.
fn cursor_edge_bubbles(conn: &Connection, earliest: bool) -> std::collections::HashMap<String, Value> {
    let agg = if earliest { "MIN" } else { "MAX" };
    let sql = format!(
        r#"
        SELECT {CURSOR_BUBBLE_COMPOSER_ID_SQL} AS cid, value
        FROM cursorDiskKV
        WHERE rowid IN (
            SELECT {agg}(rowid) FROM cursorDiskKV WHERE key LIKE 'bubbleId:%' GROUP BY {CURSOR_BUBBLE_COMPOSER_ID_SQL}
        )
        "#
    );
    let mut out = std::collections::HashMap::new();
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, cursor_json_column(row, 1))))
    else {
        return out;
    };
    for (cid, value) in rows.flatten() {
        if let Some(v) = value {
            out.insert(cid, v);
        }
    }
    out
}

/// List every local Cursor session. See the section doc for why this avoids
/// reading full bubble content at list time.
///
/// Unlike Claude/Codex, `range` can't be applied via a filesystem stat (every
/// session lives in one shared DB file), so it's applied after computing each
/// composer's `updated_at` from the already-cheap edge-bubble query above —
/// it narrows the *returned* list rather than the query cost itself, which is
/// already small (see the section doc's benchmark).
pub fn list_cursor_sessions(range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    list_cursor_sessions_at(&cursor_global_db_path(), range)
}

fn list_cursor_sessions_at(path: &Path, range: UsageDateRange) -> Result<Vec<SessionSummary>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let Some(conn) = open_cursor_db_readonly(path) else {
        return Ok(Vec::new());
    };
    let cutoff = range_cutoff_iso8601(range);

    let counts = cursor_bubble_counts(&conn);
    let first_bubbles = cursor_edge_bubbles(&conn, true);
    let last_bubbles = cursor_edge_bubbles(&conn, false);

    let Ok(mut stmt) = conn.prepare("SELECT value FROM cursorDiskKV WHERE key LIKE 'composerData:%'") else {
        return Ok(Vec::new());
    };
    let Ok(rows) = stmt.query_map([], |row| Ok(cursor_json_column(row, 0))) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for composer in rows.flatten().flatten() {
        let Some(composer_id) = composer.get("composerId").and_then(Value::as_str) else {
            continue;
        };
        let composer_id = composer_id.to_string();
        let first = first_bubbles.get(&composer_id);
        let last = last_bubbles.get(&composer_id);

        // Bubble `createdAt` is already ISO 8601; composer `createdAt` is
        // epoch milliseconds, used only when a composer has no bubbles yet
        // (an empty/draft composer).
        let started_at = first
            .and_then(|b| b.get("createdAt"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                composer
                    .get("createdAt")
                    .and_then(Value::as_i64)
                    .map(epoch_millis_to_iso8601)
            });
        let updated_at = last
            .and_then(|b| b.get("createdAt"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| started_at.clone());

        // A composer with no resolvable timestamp at all is excluded once a
        // narrower-than-AllTime range is active, same as an out-of-range one.
        if let Some(cutoff) = &cutoff {
            match &updated_at {
                Some(u) if u.as_str() >= cutoff.as_str() => {}
                _ => continue,
            }
        }

        let workspace = first
            .and_then(|b| b.get("workspaceUris"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
            .and_then(cursor_workspace_from_uri);

        let title = cursor_title(&composer, first);

        out.push(SessionSummary {
            session_key: format!("cursor:{composer_id}"),
            tool: ToolId::Cursor,
            title,
            workspace,
            git_branch: None,
            model: None,
            started_at,
            updated_at,
            message_count: counts.get(&composer_id).copied().unwrap_or(0),
            source_path: composer_id,
        });
    }
    Ok(out)
}

fn cursor_workspace_from_uri(uri: &str) -> Option<String> {
    let raw = uri.strip_prefix("file://")?;
    Some(tildify(Path::new(&percent_decode(raw))))
}

/// Minimal `%XX` decoder — Cursor's `workspaceUris` are `file://` URIs whose
/// only encoded characters are ordinary path characters (spaces, etc.), so a
/// full RFC 3986 decoder isn't worth a new dependency for this.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Prefer the first bubble's own text (the real opening message) over the
/// composer's `text` field, which reflects the input box's last-saved draft
/// and is not reliably the conversation's first message.
fn cursor_title(composer: &Value, first_bubble: Option<&Value>) -> String {
    if let Some(bubble) = first_bubble {
        if bubble.get("type").and_then(Value::as_i64) == Some(1) {
            if let Some(text) = bubble.get("text").and_then(Value::as_str) {
                if let Some(title) = first_line_title(text) {
                    return title;
                }
            }
        }
    }
    if let Some(text) = composer.get("text").and_then(Value::as_str) {
        if let Some(title) = first_line_title(text) {
            return title;
        }
    }
    "Untitled session".to_string()
}

/// Convert Unix epoch milliseconds (composer-level `createdAt`) to the same
/// `YYYY-MM-DDTHH:MM:SSZ` shape `managed_copy::now_iso8601` produces for
/// "now", so Cursor's drafts-only composers still sort against Claude/Codex's
/// ISO 8601 strings. Mirrors that function's dependency-free civil-from-days
/// math for an arbitrary instant instead of only the current time.
fn epoch_millis_to_iso8601(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Read one Cursor session's full transcript on demand: every bubble for
/// `composer_id`, ordered by insertion (`rowid`), skipping bubbles with no
/// renderable text. This is the only place per-session bubble *content* is
/// read — list/summarize deliberately avoid it (see the section doc).
///
/// v1 renders only bubbles with non-empty `text`. Cursor's agentic turns can
/// carry all their real content in tool-call/diff fields with an empty `text`
/// summary; those turns are omitted here rather than guessed at, so a Cursor
/// transcript may show fewer turns than the session actually contained.
fn read_cursor_transcript(composer_id: &str) -> Result<Vec<SessionMessage>> {
    read_cursor_transcript_at(&cursor_global_db_path(), composer_id)
}

fn read_cursor_transcript_at(path: &Path, composer_id: &str) -> Result<Vec<SessionMessage>> {
    let Some(conn) = open_cursor_db_readonly(path) else {
        return Err(CoreError::SessionSourceUnavailable(
            path.to_string_lossy().into_owned(),
        ));
    };
    let pattern = format!("bubbleId:{composer_id}:%");
    let Ok(mut stmt) = conn.prepare("SELECT value FROM cursorDiskKV WHERE key LIKE ?1 ORDER BY rowid")
    else {
        return Ok(Vec::new());
    };
    let Ok(rows) = stmt.query_map(params![pattern], |row| Ok(cursor_json_column(row, 0))) else {
        return Ok(Vec::new());
    };

    let mut messages = Vec::new();
    for bubble in rows.flatten().flatten() {
        let role = match bubble.get("type").and_then(Value::as_i64) {
            Some(1) => SessionRole::User,
            Some(2) => SessionRole::Assistant,
            _ => continue,
        };
        let Some(text) = bubble.get("text").and_then(Value::as_str) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let timestamp = bubble
            .get("createdAt")
            .and_then(Value::as_str)
            .map(str::to_string);
        messages.push(SessionMessage {
            role,
            text: text.to_string(),
            tool_name: None,
            timestamp,
        });
    }
    Ok(messages)
}

// ---- shared helpers ---------------------------------------------------------

/// Recursively collect `.jsonl` files under `dir`, bounded to `max_depth`
/// levels so an unexpected deep tree cannot spin forever. Missing directories
/// yield an empty result rather than an error.
fn collect_jsonl_files(dir: &Path, max_depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if max_depth > 0 {
                collect_jsonl_files(&path, max_depth - 1, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn claude_fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let cwd = home_dir().join("demo").to_string_lossy().into_owned();
        let lines = [
            format!(
                r#"{{"type":"user","cwd":"{cwd}","gitBranch":"main","timestamp":"2026-07-01T10:00:00Z","slug":"fallback-slug","message":{{"role":"user","content":"<command-message>feature-dev</command-message>\n<command-args>Add the sessions tab please</command-args>"}}}}"#
            ),
            r#"{"type":"assistant","timestamp":"2026-07-01T10:00:05Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"Sure, on it."},{"type":"tool_use","id":"t1","name":"Read","input":{"file":"a.rs"}}]}}"#.to_string(),
            r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-07-01T10:00:06Z"}"#.to_string(),
        ];
        write_file(&path, &lines.join("\n"));
        (dir, path)
    }

    #[test]
    fn summarize_claude_session_extracts_metadata_and_title() {
        let (_dir, path) = claude_fixture();

        let summary = summarize_claude_session(&path).expect("summary");

        assert_eq!(summary.tool, ToolId::Claude);
        assert_eq!(summary.title, "Add the sessions tab please");
        assert_eq!(summary.workspace.as_deref(), Some("~/demo"));
        assert_eq!(summary.git_branch.as_deref(), Some("main"));
        assert_eq!(summary.started_at.as_deref(), Some("2026-07-01T10:00:00Z"));
        assert_eq!(summary.updated_at.as_deref(), Some("2026-07-01T10:00:06Z"));
        assert_eq!(summary.message_count, 2);
        assert!(summary.session_key.starts_with("claude:"));
    }

    #[test]
    fn claude_title_falls_back_to_slug_when_no_user_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        write_file(
            &path,
            r#"{"type":"assistant","slug":"my-slug","timestamp":"2026-07-01T10:00:00Z","message":{"role":"assistant","content":"hello"}}"#,
        );

        let summary = summarize_claude_session(&path).expect("summary");

        assert_eq!(summary.title, "my-slug");
    }

    #[test]
    fn read_claude_transcript_extracts_text_and_tool_use() {
        let (_dir, path) = claude_fixture();

        let messages = read_claude_transcript(&path).unwrap();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, SessionRole::User);
        assert!(messages[0].text.contains("Add the sessions tab please"));
        assert_eq!(messages[1].role, SessionRole::Assistant);
        assert_eq!(messages[1].text, "Sure, on it.");
        assert_eq!(messages[2].role, SessionRole::Tool);
        assert_eq!(messages[2].tool_name.as_deref(), Some("Read"));
    }

    fn codex_fixture(id: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("sessions/2026/07/01")
            .join(format!("rollout-{id}.jsonl"));
        let cwd = home_dir().join("demo").to_string_lossy().into_owned();
        let lines = [
            format!(
                r#"{{"type":"session_meta","timestamp":"2026-07-01T09:00:00Z","payload":{{"id":"{id}","timestamp":"2026-07-01T09:00:00Z","cwd":"{cwd}","model_provider":"openai","git":{{"branch":"main"}}}}}}"#
            ),
            r#"{"type":"event_msg","timestamp":"2026-07-01T09:00:01Z","payload":{"type":"user_message","message":"hello"}}"#.to_string(),
            r#"{"type":"response_item","timestamp":"2026-07-01T09:00:02Z","payload":{"type":"reasoning"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-07-01T09:00:03Z","payload":{"type":"agent_message","message":"Hello back"}}"#.to_string(),
        ];
        write_file(&path, &lines.join("\n"));
        (dir, path)
    }

    #[test]
    fn summarize_codex_session_uses_index_title_and_git_branch() {
        let (_dir, path) = codex_fixture("019cae29-bb0b-7de1-94d8-2a7882786aa8");
        let mut titles = std::collections::HashMap::new();
        titles.insert(
            "019cae29-bb0b-7de1-94d8-2a7882786aa8".to_string(),
            "Locate AI message override point".to_string(),
        );

        let summary = summarize_codex_session(&path, &titles).expect("summary");

        assert_eq!(summary.title, "Locate AI message override point");
        assert_eq!(summary.workspace.as_deref(), Some("~/demo"));
        assert_eq!(summary.git_branch.as_deref(), Some("main"));
        assert_eq!(summary.model.as_deref(), Some("openai"));
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.updated_at.as_deref(), Some("2026-07-01T09:00:03Z"));
    }

    #[test]
    fn summarize_codex_session_falls_back_to_first_user_message() {
        let (_dir, path) = codex_fixture("no-index-entry");

        let summary = summarize_codex_session(&path, &std::collections::HashMap::new())
            .expect("summary");

        assert_eq!(summary.title, "hello");
    }

    #[test]
    fn read_codex_transcript_skips_non_message_events() {
        let (_dir, path) = codex_fixture("transcript-test");

        let messages = read_codex_transcript(&path).unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, SessionRole::User);
        assert_eq!(messages[0].text, "hello");
        assert_eq!(messages[1].role, SessionRole::Assistant);
        assert_eq!(messages[1].text, "Hello back");
    }

    #[test]
    fn filter_sessions_matches_tool_workspace_and_query() {
        let sessions = vec![
            SessionSummary {
                session_key: "claude:a".into(),
                tool: ToolId::Claude,
                title: "Fix dark scheme".into(),
                workspace: Some("~/demo".into()),
                git_branch: None,
                model: None,
                started_at: None,
                updated_at: Some("2026-07-01T10:00:00Z".into()),
                message_count: 1,
                source_path: "/tmp/a.jsonl".into(),
            },
            SessionSummary {
                session_key: "codex:b".into(),
                tool: ToolId::Codex,
                title: "Unrelated work".into(),
                workspace: Some("~/other".into()),
                git_branch: None,
                model: None,
                started_at: None,
                updated_at: Some("2026-07-01T09:00:00Z".into()),
                message_count: 1,
                source_path: "/tmp/b.jsonl".into(),
            },
        ];

        let by_tool = filter_sessions(
            sessions.clone(),
            &SessionListFilter {
                tools: Some(vec![ToolId::Claude]),
                ..Default::default()
            },
        );
        assert_eq!(by_tool.len(), 1);
        assert_eq!(by_tool[0].session_key, "claude:a");

        let by_query = filter_sessions(
            sessions.clone(),
            &SessionListFilter {
                query: Some("dark".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_query.len(), 1);
        assert_eq!(by_query[0].session_key, "claude:a");

        // A query under two characters matches everything (mirrors the skills
        // search convention of not firing on a bare keystroke).
        let by_short_query = filter_sessions(
            sessions.clone(),
            &SessionListFilter {
                query: Some("d".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_short_query.len(), 2);

        let by_workspace = filter_sessions(
            sessions,
            &SessionListFilter {
                workspace: Some("~/other".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_workspace.len(), 1);
        assert_eq!(by_workspace[0].session_key, "codex:b");
    }

    #[test]
    fn read_session_transcript_reports_missing_source() {
        let err = read_session_transcript(ToolId::Claude, "/no/such/path.jsonl").unwrap_err();
        assert!(matches!(err, CoreError::SessionSourceUnavailable(_)));
    }

    #[test]
    fn read_session_transcript_reports_unsupported_tool() {
        // No session reader exists for tools other than Claude/Codex/Cursor;
        // this must report the same typed error, never panic/unreachable.
        let err = read_session_transcript(ToolId::Kiro, "anything").unwrap_err();
        assert!(matches!(err, CoreError::SessionSourceUnavailable(_)));
    }

    // ---- Cursor -------------------------------------------------------

    /// Build a `cursorDiskKV` fixture matching the real schema and content
    /// shapes captured from a live Cursor install: `composerData:<id>` rows
    /// plus `bubbleId:<composerId>:<bubbleId>` rows inserted in conversation
    /// order (so `rowid` ordering matches insertion order, as in production).
    fn cursor_fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
            params![
                "composerData:1fb84d21-bc1a-4898-ba42-1faa1e3d753f",
                r#"{"composerId":"1fb84d21-bc1a-4898-ba42-1faa1e3d753f","text":"stale draft, not the real title","createdAt":1768293815516}"#,
            ],
        )
        .unwrap();
        // A drafted-but-never-sent composer: no bubbles at all.
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
            params![
                "composerData:00000000-0000-0000-0000-000000000000",
                r#"{"composerId":"00000000-0000-0000-0000-000000000000","text":"/wealth","createdAt":1770622649445}"#,
            ],
        )
        .unwrap();
        // A tombstoned/null row, as seen in a real profile — must not crash.
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES (?1, NULL)",
            params!["composerData:dead"],
        )
        .unwrap();

        let bubbles = [
            (
                "9002a995-e4ab-4af7-b1b7-fd0dd4336589",
                r#"{"type":1,"text":"Why is my npm publish failing?","createdAt":"2026-01-19T03:50:08.237Z","workspaceUris":["file:///tmp/demo%20space"]}"#,
            ),
            (
                "3bcdf71e-dc75-4065-b5c1-053434115587",
                r#"{"type":2,"text":"","createdAt":"2026-01-19T03:50:12.885Z"}"#,
            ),
            (
                "7be676d7-47f2-4dd9-beb4-2e2ca1120565",
                r#"{"type":2,"text":"Your npm auth token is not set.","createdAt":"2026-01-19T03:50:18.267Z"}"#,
            ),
        ];
        for (bubble_id, value) in bubbles {
            conn.execute(
                "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                params![
                    format!("bubbleId:1fb84d21-bc1a-4898-ba42-1faa1e3d753f:{bubble_id}"),
                    value,
                ],
            )
            .unwrap();
        }
        (dir, path)
    }

    #[test]
    fn list_cursor_sessions_extracts_title_workspace_and_counts() {
        let (_dir, path) = cursor_fixture();

        let sessions = list_cursor_sessions_at(&path, UsageDateRange::AllTime).unwrap();

        let with_bubbles = sessions
            .iter()
            .find(|s| s.session_key == "cursor:1fb84d21-bc1a-4898-ba42-1faa1e3d753f")
            .expect("session with bubbles");
        assert_eq!(with_bubbles.tool, ToolId::Cursor);
        // Title prefers the first bubble's real text over composerData's
        // stale draft-box text.
        assert_eq!(with_bubbles.title, "Why is my npm publish failing?");
        assert_eq!(with_bubbles.workspace.as_deref(), Some("/tmp/demo space"));
        // message_count is a raw bubble count (3 inserted), independent of
        // read_cursor_transcript's separate empty-text filtering (2 rendered).
        assert_eq!(with_bubbles.message_count, 3);
        assert_eq!(
            with_bubbles.started_at.as_deref(),
            Some("2026-01-19T03:50:08.237Z")
        );
        assert_eq!(
            with_bubbles.updated_at.as_deref(),
            Some("2026-01-19T03:50:18.267Z")
        );

        let draft_only = sessions
            .iter()
            .find(|s| s.session_key == "cursor:00000000-0000-0000-0000-000000000000")
            .expect("draft-only session");
        assert_eq!(draft_only.title, "/wealth");
        assert_eq!(draft_only.message_count, 0);
        assert_eq!(draft_only.workspace, None);
        // No bubbles, so falls back to the epoch-millis composer timestamp.
        assert_eq!(draft_only.started_at.as_deref(), Some("2026-02-09T07:37:29Z"));

        assert_eq!(sessions.len(), 2, "the null-value tombstone row is skipped, not counted");
    }

    #[test]
    fn read_cursor_transcript_maps_roles_and_skips_empty_text() {
        let (_dir, path) = cursor_fixture();

        let messages =
            read_cursor_transcript_at(&path, "1fb84d21-bc1a-4898-ba42-1faa1e3d753f").unwrap();

        assert_eq!(messages.len(), 2, "the empty-text assistant bubble is skipped");
        assert_eq!(messages[0].role, SessionRole::User);
        assert_eq!(messages[0].text, "Why is my npm publish failing?");
        assert_eq!(messages[0].timestamp.as_deref(), Some("2026-01-19T03:50:08.237Z"));
        assert_eq!(messages[1].role, SessionRole::Assistant);
        assert_eq!(messages[1].text, "Your npm auth token is not set.");
    }

    #[test]
    fn read_cursor_transcript_reports_missing_db() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such.vscdb");

        let err = read_cursor_transcript_at(&missing, "any-id").unwrap_err();

        assert!(matches!(err, CoreError::SessionSourceUnavailable(_)));
    }

    #[test]
    fn list_cursor_sessions_is_empty_when_db_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such.vscdb");

        assert_eq!(
            list_cursor_sessions_at(&missing, UsageDateRange::AllTime).unwrap(),
            Vec::new()
        );
    }

    #[test]
    #[ignore = "manual: reads the real local ~/Library/Application Support/Cursor/User/globalStorage/state.vscdb"]
    fn list_cursor_sessions_reads_real_local_db() {
        let sessions = list_cursor_sessions(UsageDateRange::AllTime).unwrap();
        eprintln!("real cursor sessions found: {}", sessions.len());
        assert!(!sessions.is_empty(), "expected at least one real local Cursor session");
        let with_messages = sessions.iter().find(|s| s.message_count > 0);
        if let Some(s) = with_messages {
            let transcript = read_session_transcript(ToolId::Cursor, &s.source_path).unwrap();
            eprintln!(
                "sample session '{}' ({} messages listed, {} rendered): {:?}",
                s.title,
                s.message_count,
                transcript.len(),
                s.workspace
            );
            assert!(!transcript.is_empty());
        }
    }

    #[test]
    #[ignore = "manual: measures list_all_sessions against real local Claude/Codex/Cursor stores"]
    fn list_all_sessions_last_7_days_is_faster_than_all_time() {
        let all_time_start = std::time::Instant::now();
        let all_time = list_all_sessions(UsageDateRange::AllTime).unwrap();
        let all_time_elapsed = all_time_start.elapsed();

        let weekly_start = std::time::Instant::now();
        let weekly = list_all_sessions(UsageDateRange::Last7Days).unwrap();
        let weekly_elapsed = weekly_start.elapsed();

        eprintln!(
            "AllTime: {} sessions in {:?} | Last7Days: {} sessions in {:?}",
            all_time.len(),
            all_time_elapsed,
            weekly.len(),
            weekly_elapsed
        );
        assert!(weekly.len() <= all_time.len());
    }

    #[test]
    fn epoch_millis_to_iso8601_matches_known_instant() {
        assert_eq!(epoch_millis_to_iso8601(1768293815516), "2026-01-13T08:43:35Z");
    }

    #[test]
    fn percent_decode_handles_encoded_spaces() {
        assert_eq!(percent_decode("/tmp/demo%20space"), "/tmp/demo space");
        assert_eq!(percent_decode("/plain/path"), "/plain/path");
    }

    // ---- date-range filtering -------------------------------------------

    #[test]
    fn range_days_and_cutoff_match_all_time_has_no_cutoff() {
        assert_eq!(range_days(UsageDateRange::Today), Some(1));
        assert_eq!(range_days(UsageDateRange::Last7Days), Some(7));
        assert_eq!(range_days(UsageDateRange::Last30Days), Some(30));
        assert_eq!(range_days(UsageDateRange::Last90Days), Some(90));
        assert_eq!(range_days(UsageDateRange::AllTime), None);
        assert!(range_cutoff_system_time(UsageDateRange::AllTime).is_none());
        assert!(range_cutoff_iso8601(UsageDateRange::AllTime).is_none());
        assert!(range_cutoff_system_time(UsageDateRange::Today).is_some());
        assert!(range_cutoff_iso8601(UsageDateRange::Today).is_some());
        assert!(range_cutoff_system_time(UsageDateRange::Last7Days).is_some());
        assert!(range_cutoff_iso8601(UsageDateRange::Last7Days).is_some());
    }

    #[test]
    fn mtime_within_range_compares_against_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        fs::write(&path, "x").unwrap();

        // AllTime (no cutoff) always matches.
        assert!(mtime_within_range(&path, None));
        // A cutoff clearly before the file's just-now mtime matches.
        let past_cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        assert!(mtime_within_range(&path, Some(past_cutoff)));
        // A cutoff clearly after the file's just-now mtime does not.
        let future_cutoff = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        assert!(!mtime_within_range(&path, Some(future_cutoff)));
        // A file that can't be stat'd is kept, not silently dropped.
        assert!(mtime_within_range(&dir.path().join("missing.txt"), Some(future_cutoff)));
    }

    fn backdate(path: &Path, days_ago: u64) {
        let time = std::time::SystemTime::now() - std::time::Duration::from_secs(days_ago * 86_400);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn list_claude_sessions_at_skips_files_older_than_range() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let old_path = project.join("old.jsonl");
        let fresh_path = project.join("fresh.jsonl");
        let line = r#"{"type":"user","timestamp":"2020-01-01T00:00:00Z","message":{"role":"user","content":"hi"}}"#;
        fs::write(&old_path, line).unwrap();
        fs::write(&fresh_path, line).unwrap();
        backdate(&old_path, 40);

        let within_30_days = list_claude_sessions_at(dir.path(), UsageDateRange::Last30Days).unwrap();
        assert_eq!(within_30_days.len(), 1);
        assert!(within_30_days[0].source_path.ends_with("fresh.jsonl"));

        let all_time = list_claude_sessions_at(dir.path(), UsageDateRange::AllTime).unwrap();
        assert_eq!(all_time.len(), 2, "AllTime must not apply any mtime cutoff");
    }

    #[test]
    fn list_codex_sessions_at_skips_files_older_than_range() {
        let dir = tempfile::tempdir().unwrap();
        let day_dir = dir.path().join("2026/07/01");
        fs::create_dir_all(&day_dir).unwrap();
        let old_path = day_dir.join("rollout-old.jsonl");
        let fresh_path = day_dir.join("rollout-fresh.jsonl");
        let meta = |id: &str| {
            format!(
                r#"{{"type":"session_meta","timestamp":"2020-01-01T00:00:00Z","payload":{{"id":"{id}","timestamp":"2020-01-01T00:00:00Z","cwd":"/tmp"}}}}"#
            )
        };
        fs::write(&old_path, meta("old-id")).unwrap();
        fs::write(&fresh_path, meta("fresh-id")).unwrap();
        backdate(&old_path, 40);

        let within_30_days = list_codex_sessions_at(dir.path(), UsageDateRange::Last30Days).unwrap();
        assert_eq!(within_30_days.len(), 1);
        assert_eq!(within_30_days[0].session_key, "codex:fresh-id");

        let all_time = list_codex_sessions_at(dir.path(), UsageDateRange::AllTime).unwrap();
        assert_eq!(all_time.len(), 2, "AllTime must not apply any mtime cutoff");
    }

    #[test]
    fn list_cursor_sessions_at_excludes_sessions_older_than_range() {
        let (_dir, path) = cursor_fixture();

        // The fixture's bubbles are dated 2026-01-19 — long before "now" in any
        // real test run — so a narrow range must exclude the whole composer.
        let recent = list_cursor_sessions_at(&path, UsageDateRange::Last7Days).unwrap();
        assert!(recent.is_empty(), "fixture sessions are far outside a 7-day window");

        let all_time = list_cursor_sessions_at(&path, UsageDateRange::AllTime).unwrap();
        assert_eq!(all_time.len(), 2);
    }
}
