use super::*;
use crate::agent_spec::AgentSpec;

fn store() -> (tempfile::TempDir, SuiteStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = SuiteStore::with_path(dir.path().join(".agentic-suites.json"));
    (dir, store)
}

fn create(name: &str, agent: Option<AgentSpec>) -> SuiteCreateInput {
    SuiteCreateInput {
        name: name.to_string(),
        description: None,
        capabilities: vec!["skill:a".into()],
        agent,
    }
}

fn cto_agent() -> AgentSpec {
    AgentSpec {
        emoji: Some("⚒️".into()),
        instructions: Some("Lead the build.".into()),
        required_clis: vec!["gh".into()],
    }
}

#[test]
fn create_round_trips_the_agent_block() {
    let (_d, store) = store();
    let made = store.create(create("cto", Some(cto_agent()))).unwrap();

    let fetched = store.get(&made.id).unwrap().unwrap();
    assert_eq!(fetched.agent, Some(cto_agent()));
    let raw = fs::read_to_string(&store.path).unwrap();
    assert!(raw.contains("\"requiredClis\""), "{raw}");
}

#[test]
fn suite_without_agent_writes_no_agent_key() {
    let (_d, store) = store();
    store.create(create("plain", None)).unwrap();
    let raw = fs::read_to_string(&store.path).unwrap();
    assert!(!raw.contains("\"agent\""), "{raw}");
}

#[test]
fn legacy_file_without_agent_loads_as_none() {
    let (_d, store) = store();
    fs::write(
        &store.path,
        r#"{ "version": 1, "suites": [
            { "id": "s1", "name": "coding", "description": null, "capabilities": [],
              "createdAt": "t", "updatedAt": "t" } ] }"#,
    )
    .unwrap();
    assert_eq!(store.get("s1").unwrap().unwrap().agent, None);
}

#[test]
fn update_sets_then_clears_the_agent_block() {
    let (_d, store) = store();
    let made = store.create(create("cto", None)).unwrap();

    let set = SuiteUpdateInput {
        agent: Some(Some(cto_agent())),
        ..Default::default()
    };
    assert_eq!(
        store.update(&made.id, set).unwrap().agent,
        Some(cto_agent())
    );

    let clear = SuiteUpdateInput {
        agent: Some(None),
        ..Default::default()
    };
    assert_eq!(store.update(&made.id, clear).unwrap().agent, None);
}

#[test]
fn update_without_agent_field_keeps_the_agent_block() {
    let (_d, store) = store();
    let made = store.create(create("cto", Some(cto_agent()))).unwrap();

    let rename = SuiteUpdateInput {
        name: Some("cto2".into()),
        ..Default::default()
    };
    assert_eq!(
        store.update(&made.id, rename).unwrap().agent,
        Some(cto_agent())
    );
}

#[test]
fn update_input_reads_null_agent_as_clear_and_absent_as_unchanged() {
    let clear: SuiteUpdateInput = serde_json::from_str(r#"{ "agent": null }"#).unwrap();
    assert_eq!(clear.agent, Some(None));
    let unchanged: SuiteUpdateInput = serde_json::from_str("{}").unwrap();
    assert_eq!(unchanged.agent, None);
}

#[test]
fn create_rejects_an_agent_over_the_limits() {
    let (_d, store) = store();
    let bad = AgentSpec {
        emoji: Some("x".repeat(17)),
        ..Default::default()
    };
    assert!(matches!(
        store.create(create("cto", Some(bad))),
        Err(CoreError::InvalidAgent(_))
    ));
    assert!(store.list().unwrap().is_empty(), "nothing written");
}

#[test]
fn update_rejects_an_agent_over_the_limits() {
    let (_d, store) = store();
    let made = store.create(create("cto", None)).unwrap();
    let bad = SuiteUpdateInput {
        agent: Some(Some(AgentSpec {
            required_clis: (0..21).map(|i| format!("c{i}")).collect(),
            ..Default::default()
        })),
        ..Default::default()
    };
    assert!(matches!(
        store.update(&made.id, bad),
        Err(CoreError::InvalidAgent(_))
    ));
    assert_eq!(store.get(&made.id).unwrap().unwrap().agent, None);
}
