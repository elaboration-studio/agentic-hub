use std::fs;
use std::time::{Duration, SystemTime};

use super::test_fixture::*;
use super::*;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn entries(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn second_build_reuses_the_existing_bundle() {
    let f = Fixture::new();
    let first = f.build(CTO, Harness::Cursor);
    let sentinel = first.root.join("sentinel");
    fs::write(&sentinel, "left by the first build").unwrap();

    let second = f.build(CTO, Harness::Cursor);
    assert_eq!(first, second);
    assert!(sentinel.exists(), "bundle was not rebuilt");
}

#[test]
fn reuse_touches_the_version_dir_mtime() {
    let f = Fixture::new();
    let first = f.build(CTO, Harness::Cursor);
    let version_dir = first.root.parent().unwrap().to_path_buf();
    let old = SystemTime::now() - 3 * DAY;
    set_mtime(&version_dir, old);

    f.build(CTO, Harness::Cursor);
    let touched = fs::metadata(&version_dir).unwrap().modified().unwrap();
    assert!(touched > old + DAY, "version dir mtime refreshed on reuse");
}

#[test]
fn reuse_reprobes_required_clis() {
    let f = Fixture::new();
    f.build(CTO, Harness::Cursor);
    let nothing_installed = |_: &str| false;
    let ctx = BundleContext {
        probe: &nothing_installed,
        ..f.ctx()
    };
    let reused = build_bundle(&ctx, CTO, Harness::Cursor).unwrap();
    assert!(reused.required_clis.iter().all(|c| !c.installed));
}

#[test]
fn concurrent_builds_of_one_version_both_succeed_with_one_dir() {
    let f = Fixture::new();
    let ctx = f.ctx();
    let built: Vec<BundleManifest> = std::thread::scope(|s| {
        // Spawn all before joining any, so the builds overlap.
        let mut handles = Vec::new();
        for _ in 0..8 {
            handles.push(s.spawn(|| build_bundle(&ctx, CTO, Harness::Cursor)));
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect()
    });
    assert!(built.iter().all(|m| m == &built[0]));
    let version_dir = built[0].root.parent().unwrap();
    assert_eq!(
        entries(version_dir),
        vec!["cursor"],
        "no temp dirs left behind"
    );
}

#[test]
fn losing_the_publish_race_returns_the_winner_and_drops_the_stage() {
    let f = Fixture::new();
    let winner = f.build(CTO, Harness::Cursor);
    let stage = winner.root.with_file_name("cursor.tmp-loser");
    fs::create_dir_all(stage.join("plugin")).unwrap();
    fs::write(stage.join(MANIFEST_FILE), "{}").unwrap();

    let published = publish(&stage, &winner.root).unwrap();
    assert_eq!(published, Some(winner));
    assert!(!stage.exists());
}

#[test]
fn harnesses_of_one_version_share_the_version_dir() {
    let f = Fixture::new();
    let cursor = f.build(CTO, Harness::Cursor);
    let codex = f.build(CTO, Harness::Codex);
    assert_eq!(cursor.version, codex.version);
    assert_eq!(
        entries(cursor.root.parent().unwrap()),
        vec!["codex", "cursor"]
    );
}

#[test]
fn gc_deletes_only_stale_sibling_versions() {
    let f = Fixture::new();
    let suite_dir = f.bundles_root().join(CTO);
    let now = SystemTime::now();
    for (name, age) in [("stalever0001", 8 * DAY), ("freshver0001", DAY)] {
        fs::create_dir_all(suite_dir.join(name).join("cursor")).unwrap();
        set_mtime(&suite_dir.join(name), now - age);
    }

    let m = f.build(CTO, Harness::Cursor);
    let mut expected = vec!["freshver0001".to_string(), m.version];
    expected.sort();
    assert_eq!(entries(&suite_dir), expected);
}

#[test]
fn gc_never_deletes_the_current_version() {
    let f = Fixture::new();
    let current = f.build(CTO, Harness::Cursor).version;
    let suite_dir = f.bundles_root().join(CTO);
    fs::create_dir_all(suite_dir.join("otherver0001")).unwrap();

    let far_future = BundleContext {
        now: SystemTime::now() + 30 * DAY,
        ..f.ctx()
    };
    build_bundle(&far_future, CTO, Harness::Codex).unwrap();
    assert_eq!(entries(&suite_dir), vec![current]);
}

#[test]
fn gc_leaves_other_suites_alone() {
    let f = Fixture::new();
    let other = f.bundles_root().join("other-suite").join("oldver000001");
    fs::create_dir_all(&other).unwrap();
    set_mtime(&other, SystemTime::now() - 30 * DAY);

    f.build(CTO, Harness::Cursor);
    assert!(other.exists());
}

#[test]
fn invalid_suite_id_is_rejected_before_any_io() {
    let f = Fixture::new();
    let err = build_bundle(&f.ctx(), "../escape", Harness::Cursor).unwrap_err();
    assert!(matches!(err, BundleError::InvalidId(_)));
    assert!(!f.bundles_root().exists());
}

#[test]
fn unknown_suite_is_not_found_and_writes_nothing() {
    let f = Fixture::new();
    let err = build_bundle(&f.ctx(), "no-such-suite", Harness::Cursor).unwrap_err();
    assert!(matches!(err, BundleError::NotFound(ref id) if id == "no-such-suite"));
    assert_eq!(err.to_string(), "No suite with id no-such-suite");
    assert!(!f.bundles_root().exists());
}
