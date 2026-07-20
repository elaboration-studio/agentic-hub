//! Local loopback collector for opt-in usage tracing.

#[cfg(test)]
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentic_core::adapter_registry;
use agentic_core::hook_sync::{self, HookCanonicalEvent};
#[cfg(test)]
use agentic_core::managed_copy::now_iso8601;
use agentic_core::model::{
    CapabilityItem, CapabilityKind, ToolId, UsageDashboard, UsageDateRange, UsageStats,
};
use agentic_core::settings::Settings;
#[cfg(test)]
use agentic_core::UsageEventInput;
use agentic_core::{
    usage_tracer_enabled, usage_tracer_hook_dir, usage_tracer_hook_id, usage_tracer_item,
    usage_tracer_manifest, usage_tracer_root, HookManifest, UsageStore, USAGE_TRACER_SCRIPT,
    USAGE_TRACER_TOOLS,
};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

use crate::usage_attribution::{AttributionState, NormalizedBatch};
use crate::usage_catalog::{attribute_batch, reconcile_existing_usage};

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
    if request.method != "POST" || request.path != "/events" {
        write_response(&mut stream, 404, "not found").await?;
        return Ok(());
    }
    let (status, body) =
        handle_request_with_persist(request, token.as_str(), &attribution, persist_event_batch);
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
    if !matches!(source_tool, "cursor" | "claude" | "codex") {
        return (400, "unknown source tool");
    }
    let batch = attribution.normalize(&raw, source_tool);
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
#[cfg(test)]
use hooks::write_tracer_manifest;
pub use hooks::{sync_tracer_hooks, synced_tracer_tools};

#[cfg(test)]
mod tests;
