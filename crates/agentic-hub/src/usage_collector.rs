//! Local loopback collector for opt-in usage tracing.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentic_core::adapter_registry;
use agentic_core::hook_sync::{self, HookCanonicalEvent};
use agentic_core::model::{
    CapabilityItem, CapabilityKind, ToolId, UsageDashboard, UsageDateRange, UsageStats,
};
use agentic_core::settings::Settings;
use agentic_core::{
    usage_tracer_enabled, usage_tracer_hook_dir, usage_tracer_hook_id, usage_tracer_item,
    usage_tracer_manifest, usage_tracer_root, HookManifest, UsageRunScope, UsageStore,
    USAGE_TRACER_SCRIPT, USAGE_TRACER_TOOLS,
};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

use crate::usage_attribution::{is_traced_source_tool, AttributionState, NormalizedBatch};
use crate::usage_catalog::{attribute_batch, reconcile_existing_usage};

const HEADER_END: &[u8] = b"\r\n\r\n";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const TOKEN_HEADER: &str = "x-agentic-hub-token";
const SOURCE_TOOL_HEADER: &str = "x-agentic-hub-source-tool";
const RUN_ID_HEADER: &str = "x-agentic-hub-run-id";
const TASK_ID_HEADER: &str = "x-agentic-hub-task-id";
const AGENT_REF_HEADER: &str = "x-agentic-hub-agent-ref";
const AGENT_VERSION_HEADER: &str = "x-agentic-hub-agent-version";
const HEALTH_PATH: &str = "/health";
const RUNS_PATH: &str = "/runs";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(1);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
const HEALTH_RECOVERY_ATTEMPTS: u8 = 3;
const HEALTH_RECOVERY_DELAY: Duration = Duration::from_secs(5);
const COLLECTOR_STOP_DELAY: Duration = Duration::from_millis(100);
const SPOOL_DRAIN_INTERVAL: Duration = Duration::from_secs(30);

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
    pub tool_diagnostics: Vec<UsageToolTracingDiagnostic>,
}

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageToolTracingDiagnostic {
    pub tool: ToolId,
    pub hook_installed: bool,
    pub last_captured_at: Option<String>,
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
    attribution: Arc<AttributionState>,
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
        let summaries = store.tool_diagnostics().unwrap_or_default();
        let tool_diagnostics = USAGE_TRACER_TOOLS
            .iter()
            .copied()
            .map(|tool| {
                let summary = summaries
                    .iter()
                    .find(|summary| summary.source_tool == tool.as_str());
                UsageToolTracingDiagnostic {
                    tool,
                    hook_installed: hooks::tracer_hook_installed(settings, tool),
                    last_captured_at: summary.and_then(|summary| summary.last_captured_at.clone()),
                    resolved_event_count: summary.map_or(0, |summary| summary.resolved_event_count),
                    unresolved_event_count: summary
                        .map_or(0, |summary| summary.unresolved_event_count),
                }
            })
            .collect();
        UsageTracingStatus {
            enabled: settings.usage_tracing.enabled,
            collector_running: running,
            collector_port: settings.usage_tracing.collector_port,
            db_path: store.path().to_path_buf(),
            supported_tools: USAGE_TRACER_TOOLS.to_vec(),
            stored_event_count: store.event_count().unwrap_or(0),
            resolved_event_count: store.resolved_event_count().unwrap_or(0),
            unresolved_event_count: store.unresolved_event_count().unwrap_or(0),
            tool_diagnostics,
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
        let _ = reconcile_existing_usage(&UsageStore::new(), settings);
        let _ = UsageStore::new().purge_unattributed_events();
        let _ = spool::drain_spool(settings, &self.attribution);

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
        async_runtime::spawn(run_server(
            listener,
            token,
            Arc::clone(&self.attribution),
            rx,
        ));
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

pub fn start_spool_drainer(app: AppHandle) {
    async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(SPOOL_DRAIN_INTERVAL);
        loop {
            interval.tick().await;
            let Ok(settings) = Settings::load() else {
                continue;
            };
            if !settings.usage_tracing.enabled {
                continue;
            }
            let collector = app.state::<UsageCollectorState>();
            let _ = spool::drain_spool(&settings, &collector.attribution);
        }
    });
}

async fn run_server(
    listener: TcpListener,
    token: Arc<String>,
    attribution: Arc<AttributionState>,
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
                let attribution = Arc::clone(&attribution);
                async_runtime::spawn(async move {
                    let _ = handle_stream(stream, token, attribution).await;
                });
            }
        }
    }
}

async fn handle_stream(
    mut stream: TcpStream,
    token: Arc<String>,
    attribution: Arc<AttributionState>,
) -> io::Result<()> {
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
    if request.method == "POST" && request.path == RUNS_PATH {
        let body = handle_run_announce(&request, token.as_str());
        let status = if body == "unauthorized" {
            401
        } else if body.starts_with('{') {
            202
        } else {
            400
        };
        return write_response(&mut stream, status, &body).await;
    }
    if request.method != "POST" || request.path != "/events" {
        write_response(&mut stream, 404, "not found").await?;
        return Ok(());
    }
    let (status, body) =
        handle_request_with_persist(request, token.as_str(), &attribution, persist_event_batch);
    write_response(&mut stream, status, body).await
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RunIdentity {
    run_id: Option<String>,
    task_id: Option<String>,
    agent_ref: Option<String>,
    agent_version: Option<String>,
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    let value = headers.get(name).map(String::as_str).unwrap_or("").trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn allow_token(value: &str, min: usize, max: usize, extra: fn(char) -> bool) -> Option<String> {
    if value.len() < min || value.len() > max {
        return None;
    }
    if value.chars().all(extra) {
        Some(value.to_string())
    } else {
        None
    }
}

fn id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn ref_char(ch: char) -> bool {
    id_char(ch) || ch == '.'
}

fn parse_optional_id(value: Option<&str>) -> Result<Option<String>, ()> {
    match value {
        None => Ok(None),
        Some(value) => allow_token(value, 1, 128, id_char).ok_or(()).map(Some),
    }
}

fn parse_optional_ref(value: Option<&str>) -> Result<Option<String>, ()> {
    match value {
        None => Ok(None),
        Some(value) => allow_token(value, 1, 64, ref_char).ok_or(()).map(Some),
    }
}

fn parse_optional_version(value: Option<&str>) -> Result<Option<String>, ()> {
    match value {
        None => Ok(None),
        Some(value) if value.len() == 12 && value.chars().all(|ch| ch.is_ascii_hexdigit()) => {
            Ok(Some(value.to_ascii_lowercase()))
        }
        Some(_) => Err(()),
    }
}

fn identity_from_headers(headers: &HashMap<String, String>) -> Result<RunIdentity, ()> {
    Ok(RunIdentity {
        run_id: parse_optional_id(header_value(headers, RUN_ID_HEADER))?,
        task_id: parse_optional_id(header_value(headers, TASK_ID_HEADER))?,
        agent_ref: parse_optional_ref(header_value(headers, AGENT_REF_HEADER))?,
        agent_version: parse_optional_version(header_value(headers, AGENT_VERSION_HEADER))?,
    })
}

/// Spool meta is written by the tracer script. A bad line is dropped so the
/// skill event still lands. The hook body never supplies these fields.
fn spool_identity(
    run_id: Option<&str>,
    task_id: Option<&str>,
    agent_ref: Option<&str>,
    agent_version: Option<&str>,
) -> RunIdentity {
    RunIdentity {
        run_id: parse_optional_id(run_id).ok().flatten(),
        task_id: parse_optional_id(task_id).ok().flatten(),
        agent_ref: parse_optional_ref(agent_ref).ok().flatten(),
        agent_version: parse_optional_version(agent_version).ok().flatten(),
    }
}

fn apply_run_identity(batch: &mut NormalizedBatch, identity: &RunIdentity) {
    for occurrence in &mut batch.occurrences {
        occurrence
            .event
            .external_run_id
            .clone_from(&identity.run_id);
        occurrence
            .event
            .external_task_id
            .clone_from(&identity.task_id);
        occurrence.event.agent_ref.clone_from(&identity.agent_ref);
        occurrence
            .event
            .agent_version
            .clone_from(&identity.agent_version);
    }
}

fn tool_id_for_source(source_tool: &str) -> Option<ToolId> {
    ToolId::ALL
        .into_iter()
        .find(|tool| tool.as_str() == source_tool)
}

fn handle_run_announce(request: &HttpRequest, token: &str) -> String {
    if request.headers.get(TOKEN_HEADER).map(String::as_str) != Some(token) {
        return "unauthorized".to_string();
    }
    let raw: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(_) => return "invalid json".to_string(),
    };
    let source_tool = raw.get("sourceTool").and_then(Value::as_str).unwrap_or("");
    if !is_traced_source_tool(source_tool) {
        return "unknown source tool".to_string();
    }
    let run_id = match parse_optional_id(raw.get("runId").and_then(Value::as_str)) {
        Ok(Some(run_id)) => run_id,
        _ => return "invalid run id".to_string(),
    };
    let task_id = match parse_optional_id(raw.get("taskId").and_then(Value::as_str)) {
        Ok(task_id) => task_id,
        Err(()) => return "invalid task id".to_string(),
    };
    let agent_ref = match parse_optional_ref(raw.get("agentRef").and_then(Value::as_str)) {
        Ok(agent_ref) => agent_ref,
        Err(()) => return "invalid agent ref".to_string(),
    };
    let agent_version =
        match parse_optional_version(raw.get("agentVersion").and_then(Value::as_str)) {
            Ok(agent_version) => agent_version,
            Err(()) => return "invalid agent version".to_string(),
        };
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(_) => {
            return json!({ "tracked": false, "reason": "tracing-disabled" }).to_string();
        }
    };
    let installed = tool_id_for_source(source_tool)
        .is_some_and(|tool| hooks::tracer_hook_installed(&settings, tool));
    let (status, reason) = if !settings.usage_tracing.enabled {
        ("untracked", Some("tracing-disabled"))
    } else if !installed {
        ("untracked", Some("hook-not-installed"))
    } else {
        ("tracked", None)
    };
    // A failed insert must not answer tracked. eCanvas treats any non-202 as
    // tracker-unreachable and shows that on the run log.
    if UsageStore::new()
        .record_run_scope(&UsageRunScope {
            run_id,
            task_id,
            source_tool: source_tool.to_string(),
            agent_ref,
            agent_version,
            status: status.to_string(),
            reason: reason.map(str::to_string),
        })
        .is_err()
    {
        return "store unavailable".to_string();
    }
    match reason {
        Some(reason) => json!({ "tracked": false, "reason": reason }).to_string(),
        None => json!({ "tracked": true }).to_string(),
    }
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
    attribution: &AttributionState,
    persist: impl FnOnce(NormalizedBatch, &str) -> Result<(), String>,
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
    if !is_traced_source_tool(source_tool) {
        return (400, "unknown source tool");
    }
    let identity = match identity_from_headers(&request.headers) {
        Ok(identity) => identity,
        Err(()) => return (400, "invalid run identity"),
    };
    let mut batch = attribution.normalize(&raw, source_tool);
    apply_run_identity(&mut batch, &identity);
    let _ = persist(batch, source_tool);
    (202, "accepted")
}

fn persist_event_batch(batch: NormalizedBatch, source_tool: &str) -> Result<(), String> {
    let settings = Settings::load().map_err(|e| e.to_string())?;
    let attributed = attribute_batch(batch, &settings, source_tool);
    UsageStore::new()
        .insert_events(&attributed.events, &attributed.catalog_items)
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

pub fn query_usage_stats(items: &[CapabilityItem]) -> Result<Vec<UsageStats>, String> {
    UsageStore::new()
        .query_stats_for_items(items)
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

mod hooks;
mod spool;
#[cfg(test)]
use hooks::write_tracer_manifest;
pub use hooks::{sync_tracer_hooks, synced_tracer_tools};

#[cfg(test)]
mod tests;
