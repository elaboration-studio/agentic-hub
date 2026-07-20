use super::*;

#[test]
fn collector_accepts_valid_token_and_normalizes_event() {
    let request = test_request(
        "secret",
        json!({
            "event_type": "PostToolUse",
            "tool_name": "Skill",
            "tool_input": { "skill": "feature-dev" }
        }),
    );

    let attribution = AttributionState::default();
    let mut persisted: Option<NormalizedBatch> = None;
    let (status, body) =
        handle_request_with_persist(request, "secret", &attribution, |batch, _| {
            persisted = Some(batch);
            Ok(())
        });

    assert_eq!(status, 202);
    assert_eq!(body, "accepted");
    let batch = persisted.expect("valid request should be persisted");
    let input = &batch.occurrences[0].event;
    assert_eq!(input.source_tool, "codex");
    assert_eq!(input.event_type, "PostToolUse");
}

#[test]
fn collector_rejects_invalid_token() {
    let request = test_request("wrong", json!({ "event_type": "PostToolUse" }));

    let attribution = AttributionState::default();
    let (status, body) = handle_request_with_persist(request, "secret", &attribution, |_, _| {
        panic!("invalid token must not persist");
    });

    assert_eq!(status, 401);
    assert_eq!(body, "unauthorized");
}

#[test]
fn collector_rejects_unknown_source_tool() {
    let mut request = test_request(
        "secret",
        json!({ "tool_name": "Skill", "tool_input": { "skill": "feature-dev" } }),
    );
    request
        .headers
        .insert(SOURCE_TOOL_HEADER.to_string(), "untrusted-tool".to_string());
    let attribution = AttributionState::default();

    let (status, body) = handle_request_with_persist(request, "secret", &attribution, |_, _| {
        panic!("unknown tools must not persist");
    });

    assert_eq!((status, body), (400, "unknown source tool"));
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
        let port = listener
            .local_addr()
            .expect("foreign listener address")
            .port();
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

    let attribution = AttributionState::default();
    let (status, body) = handle_request_with_persist(request, "secret", &attribution, |_, _| {
        panic!("malformed json must not persist");
    });

    assert_eq!(status, 400);
    assert_eq!(body, "invalid json");
}

#[test]
fn collector_returns_accepted_when_storage_fails() {
    let request = test_request("secret", json!({ "event_type": "PostToolUse" }));

    let attribution = AttributionState::default();
    let (status, body) = handle_request_with_persist(request, "secret", &attribution, |_, _| {
        Err("sqlite locked".to_string())
    });

    assert_eq!(status, 202);
    assert_eq!(body, "accepted");
}

#[test]
fn tracer_manifest_adds_prompt_hooks_for_supported_tools() {
    let dir = std::env::temp_dir().join(format!("agentic-hub-usage-test-{}", uuid::Uuid::new_v4()));
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
