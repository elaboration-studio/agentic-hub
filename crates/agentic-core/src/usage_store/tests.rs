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

fn workspace_skill(id: &str, name: &str, workspace_root: &str) -> CapabilityItem {
    let mut item = skill(id, name, name);
    item.id = format!("ws::{id}");
    item.source_id = "workspace".to_string();
    item.source_label = "Workspace".to_string();
    item.source.rel_home = workspace_root.to_string();
    item
}

#[test]
fn ensure_ready_creates_schema_migration() {
    let (_dir, store) = store();

    store.ensure_ready().unwrap();

    assert!(store.has_migration(CURRENT_SCHEMA).unwrap());
}

#[test]
fn migration_from_v1_adds_scoped_identity_without_losing_history() {
    let (dir, store) = store();
    let conn = Connection::open(dir.path().join("trace.db")).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
            INSERT INTO schema_migrations VALUES (1, '2026-07-01T00:00:00Z');
            CREATE TABLE usage_events (
                event_id TEXT NOT NULL, timestamp TEXT NOT NULL, source_tool TEXT NOT NULL,
                event_type TEXT NOT NULL, tool_name TEXT, skill_name TEXT, capability_id TEXT,
                workspace TEXT, project TEXT, success INTEGER NOT NULL, duration_ms INTEGER,
                dedupe_hash TEXT NOT NULL UNIQUE, metadata_json TEXT NOT NULL
            );
            INSERT INTO usage_events VALUES (
                'legacy', '2026-07-01T00:00:00Z', 'codex', 'PostSkillUse', NULL,
                'tdd', 'skill:tdd', NULL, NULL, 1, NULL, 'legacy-hash', '{}'
            );
            "#,
    )
    .unwrap();
    drop(conn);

    store.ensure_ready().unwrap();

    let conn = store.connect().unwrap();
    let row: (String, String) = conn
        .query_row(
            "SELECT capability_id, capability_scope FROM usage_events WHERE event_id = 'legacy'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, ("skill:tdd".to_string(), "global".to_string()));
}

#[test]
fn insert_events_persists_each_distinct_skill_in_one_batch() {
    let (_dir, store) = store();
    let items = [
        skill("skill:tdd", "tdd", "tdd"),
        skill("skill:qa", "qa", "qa"),
    ];
    let events = [event("tdd", "batch-tdd"), event("qa", "batch-qa")];

    store.insert_events(&events, &items).unwrap();

    assert_eq!(store.event_count().unwrap(), 2);
}

#[test]
fn invocation_conflict_upgrades_fallback_without_double_counting() {
    let (_dir, store) = store();
    let items = [skill("skill:tdd", "tdd", "tdd")];
    let mut fallback = event("tdd", "fallback");
    fallback.event_type = "PostSkillUse".to_string();
    fallback.invocation_key = Some("turn-tdd".to_string());
    fallback.attribution_source = Some("prompt_reference".to_string());
    fallback.attribution_rank = 10;
    let mut terminal = event("tdd", "terminal");
    terminal.event_type = "PostToolUseFailure".to_string();
    terminal.success = Some(false);
    terminal.invocation_key = Some("turn-tdd".to_string());
    terminal.attribution_source = Some("skill_tool".to_string());
    terminal.attribution_rank = 100;

    store.insert_event(&fallback, &items).unwrap();
    store.insert_event(&terminal, &items).unwrap();

    let stats = store.query_stats(&["skill:tdd".to_string()]).unwrap();
    assert_eq!((stats[0].execution_count, stats[0].failure_count), (1, 1));
}

#[test]
fn scoped_query_keeps_same_named_global_and_workspace_skills_separate() {
    let (_dir, store) = store();
    let global = skill("skill:tdd", "tdd", "tdd");
    let local = workspace_skill("skill:tdd", "tdd", "~/repo");
    let resolution_items = [global.clone(), {
        let mut item = local.clone();
        item.id = "skill:tdd".to_string();
        item
    }];
    let mut global_event = event("tdd", "global-tdd");
    global_event.capability_id = Some("skill:tdd".to_string());
    let mut local_event = event("tdd", "local-tdd");
    local_event.capability_id = Some("skill:tdd".to_string());
    local_event.capability_scope = CapabilityScope::Workspace;
    local_event.workspace_root = Some("~/repo".to_string());

    store
        .insert_event(&global_event, std::slice::from_ref(&global))
        .unwrap();
    store
        .insert_event(&local_event, &resolution_items[1..])
        .unwrap();

    let stats = store.query_stats_for_items(&[global, local]).unwrap();
    assert_eq!(
        stats
            .iter()
            .map(|row| row.capability_id.as_str())
            .collect::<Vec<_>>(),
        vec!["skill:tdd", "ws::skill:tdd"]
    );
}

#[test]
fn dashboard_retains_historical_workspace_context_without_current_item() {
    let (_dir, store) = store();
    let resolution_item = skill("skill:repo-review", "repo-review", "repo-review");
    let mut input = event("repo-review", "historical-local");
    input.capability_id = Some("skill:repo-review".to_string());
    input.capability_scope = CapabilityScope::Workspace;
    input.workspace_root = Some("~/archived-repo".to_string());
    input.capability_relative_path = Some("review/repo-review".to_string());
    store
        .insert_event(&input, std::slice::from_ref(&resolution_item))
        .unwrap();

    let dashboard = store.query_dashboard(&[], UsageDateRange::AllTime).unwrap();

    assert_eq!(
        (
            dashboard.top_capabilities[0].capability_scope,
            dashboard.top_capabilities[0].workspace_root.as_deref(),
        ),
        (CapabilityScope::Workspace, Some("~/archived-repo"))
    );
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

mod resolution;
