//! Watcher-driven reconcile. Re-applies the *current* projection state from
//! fresh source content for one or all enabled tools, and auto-enables brand
//! new capabilities that land beside an already-enabled sibling.
//!
//! Reuses the existing plan/apply + rule + hook sync pipeline — no parallel
//! engine. The desktop shell's filesystem watcher calls [`reconcile_all`] after
//! each debounced change. See `docs/tech/modules/watcher.md`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::adapter_registry;
use crate::api;
use crate::applier;
use crate::hook_sync;
use crate::model::{
    ApplyResult, CapabilityItem, LinkState, SyncHooksResult, SyncRulesResult, ToolCapabilityState,
    ToolId,
};
use crate::planner;
use crate::settings::Settings;

/// Per-tool outcome of a reconcile pass, aggregated for logging.
#[derive(Debug, Clone)]
pub struct ReconcileToolOutcome {
    pub tool: ToolId,
    pub apply: ApplyResult,
    pub rules: SyncRulesResult,
    pub hooks: SyncHooksResult,
}

/// Projection states the manager created and should keep + refresh: `Enabled`
/// (no-op), `Stale` (refresh from source), `Broken` (repair). Foreign states are
/// never touched.
fn is_owned(state: LinkState) -> bool {
    matches!(
        state,
        LinkState::Enabled | LinkState::Stale | LinkState::Broken
    )
}

/// `(source_id, kind-prefix, parent-folder)` — the grouping a newcomer must
/// share with an enabled sibling to be auto-enabled.
fn folder_key(it: &CapabilityItem) -> (String, &'static str, PathBuf) {
    let parent = it
        .relative_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    (it.source_id.clone(), it.kind.id_prefix(), parent)
}

/// Compute the desired-enabled map for a reconcile pass over one tool.
///
/// - Existing items (`id` in `prev_known_ids`) keep their owned state, so the
///   plan refreshes `Stale` copies and repairs `Broken` links while leaving
///   `Disabled`/foreign items alone.
/// - Newcomers auto-enable iff a same-source, same-kind sibling in the same
///   parent folder is currently owned. A newcomer in a brand-new folder stays
///   disabled (the user opts in explicitly).
pub fn compute_reconcile_desired(
    items: &[CapabilityItem],
    states: &[ToolCapabilityState],
    prev_known_ids: &HashSet<String>,
) -> HashMap<String, bool> {
    let item_by_id: HashMap<&str, &CapabilityItem> =
        items.iter().map(|it| (it.id.as_str(), it)).collect();

    // Folders that already hold an owned (projected) item for this tool.
    let mut enabled_folders: HashSet<(String, &'static str, PathBuf)> = HashSet::new();
    for s in states {
        if is_owned(s.state) {
            if let Some(it) = item_by_id.get(s.item_id.as_str()) {
                enabled_folders.insert(folder_key(it));
            }
        }
    }

    let mut desired = HashMap::new();
    for s in states {
        let Some(it) = item_by_id.get(s.item_id.as_str()) else {
            continue;
        };
        let owned = is_owned(s.state);
        let value = if prev_known_ids.contains(&it.id) {
            owned
        } else {
            owned || enabled_folders.contains(&folder_key(it))
        };
        desired.insert(it.id.clone(), value);
    }
    desired
}

/// Inspect every applicable state (link/file/rule + hooks) for one tool.
fn states_for_tool(
    items: &[CapabilityItem],
    adapter: &adapter_registry::ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    let manifests = hook_sync::load_manifests(items);
    let mut states = planner::inspect_tool(items, adapter);
    states.extend(hook_sync::inspect_hooks(items, &manifests, adapter));
    states
}

/// Reconcile one tool: keep + refresh owned projections, auto-enable newcomers,
/// then run the plan/apply + rule + hook sync pipeline against fresh state.
pub fn reconcile_tool(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    prev_known_ids: &HashSet<String>,
) -> ReconcileToolOutcome {
    let adapter = adapter_registry::resolve(settings, tool);
    let states = states_for_tool(items, &adapter);
    let desired = compute_reconcile_desired(items, &states, prev_known_ids);

    // The watcher reconciles non-destructively; never take over real files.
    let ops = planner::build_plan(items, &adapter, &desired, false);
    let apply = applier::apply(&ops, |_, _, _, _| {});
    let rules = api::sync_rules(items, settings, tool, &desired);
    let hooks = api::sync_hooks(items, settings, tool, &desired);

    ReconcileToolOutcome {
        tool,
        apply,
        rules,
        hooks,
    }
}

/// Reconcile every enabled tool. Disabled tools are skipped (no projection).
pub fn reconcile_all(
    items: &[CapabilityItem],
    settings: &Settings,
    prev_known_ids: &HashSet<String>,
) -> Vec<ReconcileToolOutcome> {
    ToolId::ALL
        .iter()
        .filter(|&&t| adapter_registry::resolve(settings, t).enabled)
        .map(|&t| reconcile_tool(items, settings, t, prev_known_ids))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CapabilityKind;
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn item(kind: CapabilityKind, rel: &str, source_path: PathBuf) -> CapabilityItem {
        CapabilityItem {
            id: format!("{}:{}", kind.id_prefix(), rel),
            kind,
            name: rel.to_string(),
            source_path,
            relative_path: PathBuf::from(rel),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            valid: true,
            validation_errors: vec![],
        }
    }

    fn state(item_id: &str, state: LinkState) -> ToolCapabilityState {
        ToolCapabilityState {
            tool: ToolId::Codex,
            item_id: item_id.to_string(),
            target_path: PathBuf::from("/t"),
            state,
            current_link_target: None,
        }
    }

    // ---- compute_reconcile_desired truth table ----------------------------

    #[test]
    fn desired_keeps_owned_states_and_drops_the_rest() {
        let items = vec![
            item(CapabilityKind::Skill, "a", "/s/a".into()),
            item(CapabilityKind::Skill, "b", "/s/b".into()),
            item(CapabilityKind::Skill, "c", "/s/c".into()),
            item(CapabilityKind::Skill, "d", "/s/d".into()),
            item(CapabilityKind::Skill, "e", "/s/e".into()),
        ];
        let states = vec![
            state("skill:a", LinkState::Enabled),
            state("skill:b", LinkState::Stale),
            state("skill:c", LinkState::Broken),
            state("skill:d", LinkState::Disabled),
            state("skill:e", LinkState::ForeignFile),
        ];
        // All known (no newcomers).
        let known: HashSet<String> = states.iter().map(|s| s.item_id.clone()).collect();
        let desired = compute_reconcile_desired(&items, &states, &known);

        assert!(desired["skill:a"], "enabled stays on");
        assert!(desired["skill:b"], "stale stays on (refresh)");
        assert!(desired["skill:c"], "broken stays on (repair)");
        assert!(!desired["skill:d"], "disabled stays off");
        assert!(!desired["skill:e"], "foreign never taken over");
    }

    #[test]
    fn newcomer_auto_enables_only_with_enabled_sibling() {
        let items = vec![
            // Established, enabled.
            item(CapabilityKind::Skill, "dev/a", "/s/dev/a".into()),
            // Newcomer beside an enabled sibling (same folder `dev`).
            item(CapabilityKind::Skill, "dev/b", "/s/dev/b".into()),
            // Newcomer in a brand-new folder (no enabled sibling).
            item(CapabilityKind::Skill, "other/c", "/s/other/c".into()),
        ];
        let states = vec![
            state("skill:dev/a", LinkState::Enabled),
            state("skill:dev/b", LinkState::Disabled),
            state("skill:other/c", LinkState::Disabled),
        ];
        let mut known = HashSet::new();
        known.insert("skill:dev/a".to_string());

        let desired = compute_reconcile_desired(&items, &states, &known);
        assert!(desired["skill:dev/a"]);
        assert!(desired["skill:dev/b"], "sibling of enabled dev/a");
        assert!(!desired["skill:other/c"], "no enabled sibling");
    }

    #[test]
    fn newcomer_does_not_match_across_kind_or_source() {
        let mut other_source = item(CapabilityKind::Skill, "dev/x", "/s2/dev/x".into());
        other_source.source_id = "team".into();
        let items = vec![
            item(CapabilityKind::Skill, "dev/a", "/s/dev/a".into()),
            other_source,
        ];
        let states = vec![
            state("skill:dev/a", LinkState::Enabled),
            state("skill:dev/x", LinkState::Disabled),
        ];
        let mut known = HashSet::new();
        known.insert("skill:dev/a".to_string());

        let desired = compute_reconcile_desired(&items, &states, &known);
        assert!(
            !desired["skill:dev/x"],
            "different source does not count as a sibling"
        );
    }

    // ---- reconcile_tool end-to-end (tempdir) ------------------------------

    fn enable(items: &[CapabilityItem], settings: &Settings, tool: ToolId, id: &str) {
        let mut desired = HashMap::new();
        desired.insert(id.to_string(), true);
        let ops = api::plan(items, settings, tool, &desired, false);
        api::apply(&ops);
    }

    fn state_of(
        items: &[CapabilityItem],
        settings: &Settings,
        tool: ToolId,
        id: &str,
    ) -> LinkState {
        let adapter = adapter_registry::resolve(settings, tool);
        planner::inspect_tool(items, &adapter)
            .into_iter()
            .find(|s| s.item_id == id)
            .map(|s| s.state)
            .unwrap_or(LinkState::Disabled)
    }

    #[test]
    fn reconcile_refreshes_stale_managed_copy() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# keep v1");

        // Claude skills are managed copies (FileSync) — staleness is meaningful.
        // Sandbox every tool path so the rule/hook syncs in reconcile_tool can
        // never touch the real home directory.
        let settings = Settings::sandboxed(root.path(), tools.path());

        let scanned = api::scan(&settings);
        let known: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
        enable(&scanned.items, &settings, ToolId::Claude, "skill:keep");
        assert_eq!(
            state_of(&scanned.items, &settings, ToolId::Claude, "skill:keep"),
            LinkState::Enabled
        );

        // Edit the source: the managed copy is now stale.
        write(
            &root.path().join("skills/keep/SKILL.md"),
            "# keep v2 edited",
        );
        let scanned = api::scan(&settings);
        assert_eq!(
            state_of(&scanned.items, &settings, ToolId::Claude, "skill:keep"),
            LinkState::Stale
        );

        let out = reconcile_tool(&scanned.items, &settings, ToolId::Claude, &known);
        assert!(out.apply.errors.is_empty());
        assert_eq!(
            state_of(&scanned.items, &settings, ToolId::Claude, "skill:keep"),
            LinkState::Enabled,
            "reconcile refreshed the stale copy"
        );
    }

    #[test]
    fn reconcile_is_noop_when_nothing_changed() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# keep");

        let settings = Settings::sandboxed(root.path(), tools.path());

        let scanned = api::scan(&settings);
        let known: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
        enable(&scanned.items, &settings, ToolId::Codex, "skill:keep");

        let out = reconcile_tool(&scanned.items, &settings, ToolId::Codex, &known);
        assert_eq!(out.apply.created, 0);
        assert_eq!(out.apply.removed, 0);
        assert_eq!(out.apply.replaced, 0);
        assert_eq!(out.apply.refreshed, 0);
    }

    #[test]
    fn reconcile_auto_enables_newcomer_beside_enabled_sibling() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/dev/a/SKILL.md"), "# a");

        let settings = Settings::sandboxed(root.path(), tools.path());

        // Enable `dev/a`, snapshot the known ids before adding the newcomer.
        let scanned = api::scan(&settings);
        let known: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
        enable(&scanned.items, &settings, ToolId::Codex, "skill:dev/a");

        // A new skill lands beside it, plus one in a brand-new folder.
        write(&root.path().join("skills/dev/b/SKILL.md"), "# b");
        write(&root.path().join("skills/other/c/SKILL.md"), "# c");
        let scanned = api::scan(&settings);

        reconcile_tool(&scanned.items, &settings, ToolId::Codex, &known);

        assert_eq!(
            state_of(&scanned.items, &settings, ToolId::Codex, "skill:dev/b"),
            LinkState::Enabled,
            "newcomer beside enabled sibling is auto-enabled"
        );
        assert_eq!(
            state_of(&scanned.items, &settings, ToolId::Codex, "skill:other/c"),
            LinkState::Disabled,
            "newcomer in a new folder stays disabled"
        );
    }

    #[test]
    fn reconcile_refreshes_markdown_rule_body() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("rules/g/p.mdc"), "rule body v1");

        let settings = Settings::sandboxed(root.path(), tools.path());
        let instr = settings.tools.codex.instructions_path.clone().unwrap();
        fs::create_dir_all(instr.parent().unwrap()).unwrap();

        let scanned = api::scan(&settings);
        let known: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
        // Enable the rule (writes the managed block with v1 body).
        let mut desired = HashMap::new();
        desired.insert("rule:g/p.mdc".to_string(), true);
        api::sync_rules(&scanned.items, &settings, ToolId::Codex, &desired);
        assert!(fs::read_to_string(&instr).unwrap().contains("rule body v1"));

        // Edit the source rule body; reconcile must rewrite the block.
        write(&root.path().join("rules/g/p.mdc"), "rule body v2 fresh");
        let scanned = api::scan(&settings);
        reconcile_tool(&scanned.items, &settings, ToolId::Codex, &known);

        let written = fs::read_to_string(&instr).unwrap();
        assert!(written.contains("rule body v2 fresh"), "block refreshed");
        assert!(!written.contains("rule body v1"), "old body replaced");
    }

    #[test]
    fn reconcile_all_skips_disabled_tools() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        settings.tools.openclaw.enabled = false;

        let scanned = api::scan(&settings);
        let known: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
        let outcomes = reconcile_all(&scanned.items, &settings, &known);
        assert!(!outcomes.iter().any(|o| o.tool == ToolId::Openclaw));
        assert_eq!(outcomes.len(), 3, "codex + claude + cursor");
    }
}
