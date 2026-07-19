use super::*;

#[test]
fn resolve_capability_id_matches_exact_id() {
    let items = [skill("skill:dev/tdd", "tdd", "dev/tdd")];

    let id = resolve_capability_id(&items, Some("skill:dev/tdd"), Some("other"));

    assert_eq!(id.as_deref(), Some("skill:dev/tdd"));
}

#[test]
fn resolve_capability_id_matches_exact_installed_skill_id() {
    let items = [skill(
        "installed::cursor::skill:repo/local-skill",
        "local-skill",
        "repo/local-skill",
    )];

    let id = resolve_capability_id(
        &items,
        Some("installed::cursor::skill:repo/local-skill"),
        None,
    );

    assert_eq!(
        id.as_deref(),
        Some("installed::cursor::skill:repo/local-skill")
    );
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
        skill(
            "skill:root-cause-investigation",
            "root-cause-investigation",
            "root-cause-investigation",
        ),
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

    assert_eq!(
        dashboard.top_capabilities.len(),
        2,
        "both events count toward all-time top usage"
    );
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

#[test]
fn query_dashboard_today_range_excludes_events_older_than_24h() {
    let (_dir, store) = store();
    let items = [skill(
        "skill:root-cause-investigation",
        "root-cause-investigation",
        "root-cause-investigation",
    )];

    let mut recent = event("root-cause-investigation", "recent-event");
    recent.timestamp = Some(crate::managed_copy::now_iso8601());
    store.insert_event(&recent, &items).unwrap();

    let mut old = event("root-cause-investigation", "old-event");
    old.timestamp = Some("2020-01-01T00:00:00Z".to_string());
    store.insert_event(&old, &items).unwrap();

    let dashboard = store
        .query_dashboard(&items, UsageDateRange::Today)
        .unwrap();

    assert_eq!(
        dashboard.overview.terminal_events, 1,
        "UsageDateRange::Today's `-1 days` cutoff must exclude the 2020 event"
    );
}
