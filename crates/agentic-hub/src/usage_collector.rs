//! Local loopback collector for opt-in usage tracing.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentic_core::adapter_registry;
use agentic_core::hook_sync::{self, HookCanonicalEvent};
use agentic_core::managed_copy::now_iso8601;
use agentic_core::model::{
    CapabilityItem, CapabilityKind, ToolId, UsageDashboard, UsageDateRange, UsageStats,
};
use agentic_core::settings::Settings;
use agentic_core::{
    api, usage_tracer_enabled, usage_tracer_hook_dir, usage_tracer_item, usage_tracer_manifest,
    usage_tracer_root, HookManifest, UsageEventInput, UsageStore, USAGE_TRACER_SCRIPT,
    USAGE_TRACER_TOOLS,
};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

const HEADER_END: &[u8] = b"\r\n\r\n";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const TOKEN_HEADER: &str = "x-agentic-hub-token";
const SOURCE_TOOL_HEADER: &str = "x-agentic-hub-source-tool";
const HEALTH_PATH: &str = "/health";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(1);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
const HEALTH_RECOVERY_ATTEMPTS: u8 = 3;
const HEALTH_RECOVERY_DELAY: Duration = Duration::from_secs(5);
const COLLECTOR_STOP_DELAY: Duration = Duration::from_millis(100);

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTracingStatus {
    pub enabled: bool,
    pub collector_running: bool,
    pub collector_port: u16,
    pub db_path: PathBuf,
    pub supported_tools: Vec<ToolId>,
    pub stored_event_count: u32,
    pub resolved_event_count: u32,
    pub unresolved_event_count: u32,
}

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTracerHooksSyncResult {
    pub status: UsageTracingStatus,
    pub synced_tools: Vec<ToolId>,
}

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTracingHealthFailure {
    pub attempts: u8,
    pub message: String,
}

#[derive(Default)]
pub struct UsageCollectorState {
    inner: Mutex<CollectorInner>,
    recovery: AsyncMutex<()>,
    health: Mutex<CollectorHealth>,
}

#[derive(Default)]
struct CollectorInner {
    port: Option<u16>,
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
struct CollectorHealth {
    failure_notified: bool,
}

impl UsageCollectorState {
    pub async fn status(&self, settings: &Settings) -> UsageTracingStatus {
        let running = settings.usage_tracing.enabled && probe_collector(settings).await;
        if running {
            self.clear_health_failure();
        }
        let store = UsageStore::new();
        UsageTracingStatus {
            enabled: settings.usage_tracing.enabled,
            collector_running: running,
            collector_port: settings.usage_tracing.collector_port,
            db_path: store.path().to_path_buf(),
            supported_tools: USAGE_TRACER_TOOLS.to_vec(),
            stored_event_count: store.event_count().unwrap_or(0),
            resolved_event_count: store.resolved_event_count().unwrap_or(0),
            unresolved_event_count: store.unresolved_event_count().unwrap_or(0),
        }
    }

    pub fn apply_settings(&self, settings: &Settings) -> Result<(), String> {
        if settings.usage_tracing.enabled {
            self.start(settings)
        } else {
            self.stop();
            self.clear_health_failure();
            Ok(())
        }
    }

    pub async fn ensure_healthy(&self, settings: &Settings) -> Result<(), String> {
        if !settings.usage_tracing.enabled {
            self.clear_health_failure();
            return Ok(());
        }

        let _guard = self.recovery.lock().await;
        if probe_collector(settings).await {
            self.clear_health_failure();
            return Ok(());
        }

        self.restart(settings).await;
        if probe_collector(settings).await {
            self.clear_health_failure();
            return Ok(());
        }

        Err("usage collector health check failed".to_string())
    }

    pub async fn check_health(&self) -> Result<Option<UsageTracingHealthFailure>, String> {
        let _guard = self.recovery.lock().await;
        let settings = Settings::load().map_err(|error| error.to_string())?;
        self.check_health_with(
            settings,
            || Settings::load().map_err(|error| error.to_string()),
            HEALTH_RECOVERY_ATTEMPTS,
            HEALTH_RECOVERY_DELAY,
        )
        .await
    }

    async fn check_health_with<F>(
        &self,
        settings: Settings,
        load_settings: F,
        attempts: u8,
        retry_delay: Duration,
    ) -> Result<Option<UsageTracingHealthFailure>, String>
    where
        F: Fn() -> Result<Settings, String>,
    {
        if !settings.usage_tracing.enabled {
            self.clear_health_failure();
            return Ok(None);
        }
        if probe_collector(&settings).await {
            self.clear_health_failure();
            return Ok(None);
        }

        for attempt in 0..attempts {
            if attempt > 0 {
                tokio::time::sleep(retry_delay).await;
            }
            let settings = load_settings()?;
            if !settings.usage_tracing.enabled {
                self.clear_health_failure();
                return Ok(None);
            }
            self.restart(&settings).await;
            if probe_collector(&settings).await {
                self.clear_health_failure();
                return Ok(None);
            }
        }

        Ok(self
            .mark_health_failure()
            .then(|| UsageTracingHealthFailure {
                attempts,
                message: "Local usage tracing is paused. Restart Agentic Hub to resume collection."
                    .to_string(),
            }))
    }

    pub fn stop(&self) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(tx) = inner.shutdown.take() {
            let _ = tx.send(());
        }
        inner.port = None;
    }

    fn start(&self, settings: &Settings) -> Result<(), String> {
        let port = settings.usage_tracing.collector_port;
        let token = settings.usage_tracing.collector_token.clone();
        if token.trim().is_empty() {
            return Err("usage tracing token is empty".to_string());
        }

        UsageStore::new()
            .ensure_ready()
            .map_err(|e| e.to_string())?;
        let _ = UsageStore::new().purge_unattributed_events();

        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "usage collector lock poisoned".to_string())?;
        if inner.shutdown.is_some() && inner.port == Some(port) {
            return Ok(());
        }
        if let Some(tx) = inner.shutdown.take() {
            let _ = tx.send(());
        }

        let std_listener = std::net::TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("failed to bind usage collector: {e}"))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| format!("failed to configure usage collector: {e}"))?;
        let listener = TcpListener::from_std(std_listener)
            .map_err(|e| format!("failed to start usage collector: {e}"))?;
        let (tx, rx) = oneshot::channel();
        let token = Arc::new(token);
        async_runtime::spawn(run_server(listener, token, rx));
        inner.shutdown = Some(tx);
        inner.port = Some(port);
        Ok(())
    }

    async fn restart(&self, settings: &Settings) {
        self.stop();
        tokio::time::sleep(COLLECTOR_STOP_DELAY).await;
        let _ = self.start(settings);
    }

    fn clear_health_failure(&self) {
        if let Ok(mut health) = self.health.lock() {
            health.failure_notified = false;
        }
    }

    fn mark_health_failure(&self) -> bool {
        let Ok(mut health) = self.health.lock() else {
            return false;
        };
        if health.failure_notified {
            false
        } else {
            health.failure_notified = true;
            true
        }
    }
}

pub fn start_health_checker(app: AppHandle) {
    async_runtime::spawn(async move {
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + HEALTH_CHECK_INTERVAL,
            HEALTH_CHECK_INTERVAL,
        );
        loop {
            interval.tick().await;
            let collector = app.state::<UsageCollectorState>();
            if let Ok(Some(failure)) = collector.check_health().await {
                let _ = app.emit("usage-tracing-health-failed", failure);
            }
        }
    });
}

async fn run_server(
    listener: TcpListener,
    token: Arc<String>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    continue;
                };
                let token = Arc::clone(&token);
                async_runtime::spawn(async move {
                    let _ = handle_stream(stream, token).await;
                });
            }
        }
    }
}

async fn handle_stream(mut stream: TcpStream, token: Arc<String>) -> io::Result<()> {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(_) => {
            write_response(&mut stream, 400, "bad request").await?;
            return Ok(());
        }
    };
    if request.method == "GET" && request.path == HEALTH_PATH {
        let (status, body) = handle_health_request(&request, token.as_str());
        return write_response(&mut stream, status, body).await;
    }
    if request.method != "POST" || request.path != "/events" {
        write_response(&mut stream, 404, "not found").await?;
        return Ok(());
    }
    let (status, body) = handle_request_with_persist(request, token.as_str(), persist_event);
    write_response(&mut stream, status, body).await
}

fn handle_health_request(request: &HttpRequest, token: &str) -> (u16, &'static str) {
    if request.headers.get(TOKEN_HEADER).map(String::as_str) != Some(token) {
        return (401, "unauthorized");
    }
    (204, "")
}

async fn probe_collector(settings: &Settings) -> bool {
    let token = settings.usage_tracing.collector_token.trim();
    if token.is_empty() {
        return false;
    }
    let stream = match tokio::time::timeout(
        HEALTH_TIMEOUT,
        TcpStream::connect(("127.0.0.1", settings.usage_tracing.collector_port)),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        _ => return false,
    };
    let mut stream = stream;
    let request = format!(
        "GET {HEALTH_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {token}\r\nConnection: close\r\n\r\n"
    );
    if tokio::time::timeout(HEALTH_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .is_err()
    {
        return false;
    }
    let mut response = [0_u8; 64];
    match tokio::time::timeout(HEALTH_TIMEOUT, stream.read(&mut response)).await {
        Ok(Ok(read)) => response[..read].starts_with(b"HTTP/1.1 204"),
        _ => false,
    }
}

fn handle_request_with_persist(
    request: HttpRequest,
    token: &str,
    persist: impl FnOnce(UsageEventInput) -> Result<(), String>,
) -> (u16, &'static str) {
    if request.headers.get(TOKEN_HEADER).map(String::as_str) != Some(token) {
        return (401, "unauthorized");
    }

    let raw: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(_) => return (400, "invalid json"),
    };
    let source_tool = request
        .headers
        .get(SOURCE_TOOL_HEADER)
        .map(String::as_str)
        .unwrap_or("unknown");
    let input = normalize_event(raw, source_tool);
    let _ = persist(input);
    (202, "accepted")
}

fn persist_event(input: UsageEventInput) -> Result<(), String> {
    let settings = Settings::load().map_err(|e| e.to_string())?;
    let scan = api::scan(&settings);
    UsageStore::new()
        .insert_event(&input, &scan.items)
        .map_err(|e| e.to_string())
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut buf = Vec::new();
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
        let mut chunk = [0_u8; 1024];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let header_bytes = &buf[..header_end];
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut lines = header_text.split("\r\n");
    let first = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty request"))?;
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    let body_start = header_end + HEADER_END.len();
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0_u8; content_length - body.len()];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "body closed"));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(HEADER_END.len())
        .position(|window| window == HEADER_END)
}

async fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    let reason = match status {
        204 => "No Content",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await
}

fn normalize_event(raw: Value, source_tool: &str) -> UsageEventInput {
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

pub fn query_usage_stats(items: &[CapabilityItem]) -> Result<Vec<UsageStats>, String> {
    let ids: Vec<String> = items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                CapabilityKind::Skill | CapabilityKind::Command | CapabilityKind::Agent
            )
        })
        .map(|item| item.id.clone())
        .collect();
    UsageStore::new()
        .query_stats(&ids)
        .map_err(|e| e.to_string())
}

pub fn query_usage_dashboard(
    items: &[CapabilityItem],
    range: UsageDateRange,
) -> Result<UsageDashboard, String> {
    UsageStore::new()
        .query_dashboard(items, range)
        .map_err(|e| e.to_string())
}

pub fn record_command_palette_usage(
    capability_id: &str,
    pasted: bool,
    items: &[CapabilityItem],
) -> Result<(), String> {
    let settings = Settings::load().map_err(|e| e.to_string())?;
    if !settings.usage_tracing.enabled {
        return Ok(());
    }
    let exists = items
        .iter()
        .any(|item| item.kind == CapabilityKind::Command && item.id == capability_id);
    if !exists {
        return Err(format!("unknown command capability: {capability_id}"));
    }
    UsageStore::new()
        .record_palette_command_use(items, capability_id, pasted)
        .map_err(|e| e.to_string())
}

pub fn sync_tracer_hooks(settings: &Settings) -> Result<(), String> {
    ensure_tracer_script(settings)?;
    for tool in USAGE_TRACER_TOOLS {
        let enabled = usage_tracer_enabled(settings, tool);
        sync_tracer_hook_for_tool(settings, tool, enabled)?;
    }
    Ok(())
}

pub fn synced_tracer_tools(settings: &Settings) -> Vec<ToolId> {
    USAGE_TRACER_TOOLS
        .iter()
        .copied()
        .filter(|tool| usage_tracer_enabled(settings, *tool))
        .collect()
}

fn sync_tracer_hook_for_tool(
    settings: &Settings,
    tool: ToolId,
    enabled: bool,
) -> Result<(), String> {
    let hook_dir = usage_tracer_hook_dir(tool);
    fs::create_dir_all(&hook_dir).map_err(|e| e.to_string())?;
    let manifest = usage_tracer_manifest(settings, tool, &hook_dir);
    write_tracer_manifest(&hook_dir, &manifest)?;
    let item = usage_tracer_item(tool, &hook_dir);
    let adapter = adapter_registry::resolve(settings, tool);
    hook_sync::sync_single_json_hook(&adapter, &item, &manifest, enabled)
        .map(|_| ())
        .map_err(|e| e.message)
}

fn ensure_tracer_script(_settings: &Settings) -> Result<(), String> {
    let path = usage_tracer_root().join(USAGE_TRACER_SCRIPT);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        &path,
        r#"#!/bin/sh
url="$1"
token="$2"
source_tool="$3"
/usr/bin/curl -fsS --max-time 0.5 \
  -H "content-type: application/json" \
  -H "x-agentic-hub-token: ${token}" \
  -H "x-agentic-hub-source-tool: ${source_tool}" \
  --data-binary @- "${url}" >/dev/null 2>&1 || true
exit 0
"#,
    )
    .map_err(|e| e.to_string())?;
    set_executable(&path).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn write_tracer_manifest(hook_dir: &Path, manifest: &HookManifest) -> Result<(), String> {
    let events: Vec<Value> = manifest
        .events
        .iter()
        .map(|event| json!({ "name": hook_event_name(event.name), "matcher": event.matcher.as_deref() }))
        .collect();
    let body = json!({
        "$schema": "agentic-hub.hook.v1",
        "id": &manifest.id,
        "name": manifest.name.as_deref(),
        "description": manifest.description.as_deref(),
        "events": events,
        "command": &manifest.command,
        "timeout": manifest.timeout,
        "targets": manifest.effective_targets().iter().map(|tool| tool.as_str()).collect::<Vec<_>>()
    });
    fs::write(
        hook_dir.join("hook.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        ),
    )
    .map_err(|e| e.to_string())
}

fn collect_slash_capability_refs(text: &str, refs: &mut BTreeSet<String>) {
    for token in text.split_whitespace() {
        let Some(rest) = token.strip_prefix('/') else {
            continue;
        };
        let cleaned =
            rest.trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
        if cleaned.is_empty() || !is_capability_token(cleaned) {
            continue;
        }
        refs.insert(cleaned.to_string());
    }
}

fn collect_at_agent_refs(text: &str, refs: &mut BTreeSet<String>) {
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

fn collect_agent_md_path_refs(text: &str, refs: &mut BTreeSet<String>) {
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

fn agent_name_from_read_tool(raw: &Value, tool_name: Option<&str>) -> Option<String> {
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

fn tool_supports_prompt_capability_attribution(source_tool: &str) -> bool {
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

fn hook_event_name(event: HookCanonicalEvent) -> &'static str {
    match event {
        HookCanonicalEvent::PreToolUse => "PreToolUse",
        HookCanonicalEvent::PostToolUse => "PostToolUse",
        HookCanonicalEvent::PostToolUseFailure => "PostToolUseFailure",
        HookCanonicalEvent::UserPromptSubmit => "UserPromptSubmit",
        HookCanonicalEvent::UserPromptExpansion => "UserPromptExpansion",
        HookCanonicalEvent::Stop => "Stop",
        HookCanonicalEvent::SessionStart => "SessionStart",
        HookCanonicalEvent::SessionEnd => "SessionEnd",
        HookCanonicalEvent::PreCompact => "PreCompact",
        HookCanonicalEvent::PostCompact => "PostCompact",
        HookCanonicalEvent::Notification => "Notification",
        HookCanonicalEvent::PermissionRequest => "PermissionRequest",
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn collector_accepts_valid_token_and_normalizes_event() {
        let request = test_request("secret", json!({ "event_type": "PostToolUse" }));

        let mut persisted: Option<UsageEventInput> = None;
        let (status, body) = handle_request_with_persist(request, "secret", |input| {
            persisted = Some(input);
            Ok(())
        });

        assert_eq!(status, 202);
        assert_eq!(body, "accepted");
        let input = persisted.expect("valid request should be persisted");
        assert_eq!(input.source_tool, "codex");
        assert_eq!(input.event_type, "PostToolUse");
    }

    #[test]
    fn collector_rejects_invalid_token() {
        let request = test_request("wrong", json!({ "event_type": "PostToolUse" }));

        let (status, body) = handle_request_with_persist(request, "secret", |_| {
            panic!("invalid token must not persist");
        });

        assert_eq!(status, 401);
        assert_eq!(body, "unauthorized");
    }

    #[test]
    fn health_endpoint_requires_the_collector_token() {
        let request = HttpRequest {
            method: "GET".to_string(),
            path: "/health".to_string(),
            headers: HashMap::from([(TOKEN_HEADER.to_string(), "secret".to_string())]),
            body: Vec::new(),
        };

        assert_eq!(handle_health_request(&request, "secret"), (204, ""));
        assert_eq!(
            handle_health_request(&request, "wrong"),
            (401, "unauthorized")
        );
    }

    #[test]
    fn collector_status_requires_a_successful_health_probe() {
        let mut settings = Settings::default();
        settings.usage_tracing.enabled = true;
        settings.usage_tracing.collector_token = "secret".to_string();
        settings.usage_tracing.collector_port = unused_loopback_port();
        let collector = UsageCollectorState::default();

        let runtime = tokio::runtime::Runtime::new().expect("test runtime");
        runtime.block_on(async {
            let stopped = collector.status(&settings).await;
            assert!(!stopped.collector_running);

            collector.start(&settings).expect("collector starts");
            let running = collector.status(&settings).await;
            assert!(running.collector_running);
            collector.stop();
        });
    }

    #[test]
    fn ensure_healthy_restarts_a_stopped_collector() {
        let mut settings = Settings::default();
        settings.usage_tracing.enabled = true;
        settings.usage_tracing.collector_token = "secret".to_string();
        settings.usage_tracing.collector_port = unused_loopback_port();
        let collector = UsageCollectorState::default();
        let runtime = tokio::runtime::Runtime::new().expect("test runtime");

        runtime.block_on(async {
            collector.start(&settings).expect("collector starts");
            collector.stop();

            collector
                .ensure_healthy(&settings)
                .await
                .expect("collector recovers");
            assert!(collector.status(&settings).await.collector_running);
            collector.stop();
        });
    }

    #[test]
    fn health_check_retries_three_times_and_notifies_once_per_outage() {
        let collector = UsageCollectorState::default();
        let runtime = tokio::runtime::Runtime::new().expect("test runtime");

        runtime.block_on(async {
            let listener = TcpListener::bind(("127.0.0.1", 0))
                .await
                .expect("foreign listener binds");
            let port = listener.local_addr().expect("foreign listener address").port();
            let responder = tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    let mut request = [0_u8; 512];
                    let _ = stream.read(&mut request).await;
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        )
                        .await;
                }
            });
            let mut settings = Settings::default();
            settings.usage_tracing.enabled = true;
            settings.usage_tracing.collector_token = "secret".to_string();
            settings.usage_tracing.collector_port = port;

            let retry_settings = settings.clone();
            let first_retry_settings = retry_settings.clone();
            let failure = collector
                .check_health_with(
                    settings,
                    move || Ok(first_retry_settings.clone()),
                    HEALTH_RECOVERY_ATTEMPTS,
                    Duration::ZERO,
                )
                .await
                .expect("health check completes")
                .expect("first outage notifies");
            assert_eq!(failure.attempts, HEALTH_RECOVERY_ATTEMPTS);

            let repeated = collector
                .check_health_with(
                    retry_settings.clone(),
                    move || Ok(retry_settings.clone()),
                    HEALTH_RECOVERY_ATTEMPTS,
                    Duration::ZERO,
                )
                .await
                .expect("repeated health check completes");
            assert!(repeated.is_none());
            responder.abort();
        });
    }

    #[test]
    fn health_check_stays_silent_when_tracing_is_disabled_before_recovery() {
        let collector = UsageCollectorState::default();
        let mut enabled = Settings::default();
        enabled.usage_tracing.enabled = true;
        enabled.usage_tracing.collector_token = "secret".to_string();
        enabled.usage_tracing.collector_port = unused_loopback_port();
        let mut disabled = enabled.clone();
        disabled.usage_tracing.enabled = false;

        let runtime = tokio::runtime::Runtime::new().expect("test runtime");
        runtime.block_on(async {
            let failure = collector
                .check_health_with(
                    enabled,
                    move || Ok(disabled.clone()),
                    HEALTH_RECOVERY_ATTEMPTS,
                    Duration::ZERO,
                )
                .await
                .expect("health check completes");

            assert!(failure.is_none());
        });
    }

    #[test]
    fn collector_handles_malformed_json_without_persisting() {
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/events".to_string(),
            headers: HashMap::from([
                (TOKEN_HEADER.to_string(), "secret".to_string()),
                (SOURCE_TOOL_HEADER.to_string(), "codex".to_string()),
            ]),
            body: b"{not-json".to_vec(),
        };

        let (status, body) = handle_request_with_persist(request, "secret", |_| {
            panic!("malformed json must not persist");
        });

        assert_eq!(status, 400);
        assert_eq!(body, "invalid json");
    }

    #[test]
    fn collector_returns_accepted_when_storage_fails() {
        let request = test_request("secret", json!({ "event_type": "PostToolUse" }));

        let (status, body) =
            handle_request_with_persist(request, "secret", |_| Err("sqlite locked".to_string()));

        assert_eq!(status, 202);
        assert_eq!(body, "accepted");
    }

    #[test]
    fn tracer_manifest_adds_prompt_hooks_for_supported_tools() {
        let dir =
            std::env::temp_dir().join(format!("agentic-hub-usage-test-{}", uuid::Uuid::new_v4()));
        let cursor_dir = dir.join("cursor");
        let codex_dir = dir.join("codex");
        let claude_dir = dir.join("claude");
        fs::create_dir_all(&cursor_dir).unwrap();
        fs::create_dir_all(&codex_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();

        let settings = Settings::default();
        write_tracer_manifest(
            &cursor_dir,
            &usage_tracer_manifest(&settings, ToolId::Cursor, &cursor_dir),
        )
        .unwrap();
        write_tracer_manifest(
            &codex_dir,
            &usage_tracer_manifest(&settings, ToolId::Codex, &codex_dir),
        )
        .unwrap();
        write_tracer_manifest(
            &claude_dir,
            &usage_tracer_manifest(&settings, ToolId::Claude, &claude_dir),
        )
        .unwrap();

        let cursor: Value =
            serde_json::from_slice(&fs::read(cursor_dir.join("hook.json")).unwrap()).unwrap();
        let codex: Value =
            serde_json::from_slice(&fs::read(codex_dir.join("hook.json")).unwrap()).unwrap();
        let claude: Value =
            serde_json::from_slice(&fs::read(claude_dir.join("hook.json")).unwrap()).unwrap();
        assert!(has_manifest_event(&cursor, "UserPromptSubmit"));
        assert!(has_manifest_event(&codex, "UserPromptSubmit"));
        assert!(has_manifest_event(&claude, "UserPromptSubmit"));
        assert!(has_manifest_event(&claude, "UserPromptExpansion"));
        assert!(!has_manifest_event(&cursor, "UserPromptExpansion"));
        fs::remove_dir_all(&dir).unwrap();
    }

    fn has_manifest_event(manifest: &Value, name: &str) -> bool {
        manifest["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|event| event["name"].as_str())
            .any(|event_name| event_name == name)
    }

    #[test]
    fn synced_tracer_tools_lists_enabled_capture_tools_only() {
        let mut settings = Settings::default();
        settings.usage_tracing.enabled = true;
        settings.usage_tracing.capture_tools = vec![ToolId::Codex, ToolId::Cursor];
        settings.tools.codex.enabled = true;
        settings.tools.cursor.enabled = true;
        settings.tools.claude.enabled = false;

        assert_eq!(
            synced_tracer_tools(&settings),
            vec![ToolId::Codex, ToolId::Cursor]
        );
    }

    fn test_request(token: &str, body: Value) -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            path: "/events".to_string(),
            headers: HashMap::from([
                (TOKEN_HEADER.to_string(), token.to_string()),
                (SOURCE_TOOL_HEADER.to_string(), "codex".to_string()),
            ]),
            body: serde_json::to_vec(&body).expect("test json serializes"),
        }
    }

    fn unused_loopback_port() -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("bind ephemeral port")
            .local_addr()
            .expect("read ephemeral port")
            .port()
    }
}
