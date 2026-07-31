//! High-level handlers that the Tauri shell wraps 1:1 as `#[tauri::command]`s.
//!
//! These are pure orchestration over the engine modules — no Tauri types — so
//! they stay unit-testable against a tempdir. The shell layer only does payload
//! marshalling and error mapping. See `docs/tech/modules/tauri-ipc-contract.md`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::adapter_registry::{self, ProjectionMode, ResolvedAdapter};
use crate::applier;
use crate::hook_sync;
use crate::internal_hooks;
use crate::model::{
    ApplyResult, ApplySuiteResult, CapabilityItem, CapabilityKind, HookSyncOutcome, LinkState,
    PlannedOperation, RuleSyncOutcome, ScanResult, SuiteBinding, SuiteDefinition, SuiteOwnership,
    SyncHooksResult, SyncRulesResult, ToolCapabilityState, ToolId,
};
use crate::planner;
use crate::rule_sync;
use crate::scanner;
use crate::settings::{source_present, Settings};

/// Availability of one tool adapter, surfaced so the UI can disable a tool tab.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterStatus {
    pub tool: ToolId,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// Result of inspecting current per-tool state for a set of items.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub states: Vec<ToolCapabilityState>,
    pub adapter_statuses: Vec<AdapterStatus>,
}

/// Scan the source forest configured in settings (resolving the legacy single
/// root fallback). Hook items are annotated with `hook.json` validation
/// results; skill items a source root's skills.sh lock manages are annotated
/// with `locked_skills` so the Manager can badge them.
pub fn scan(settings: &Settings) -> ScanResult {
    let sources = settings.resolve_sources();
    let mut result = scanner::scan_all(&sources);
    hook_sync::annotate_validation(&mut result.items);
    internal_hooks::append_items(&mut result.items, settings);
    result.locked_skills = crate::source_skill_lock::mark_locked_library_skills(
        &sources,
        &result.items,
    );
    result
}

fn items_with_internal_hooks(items: &[CapabilityItem], settings: &Settings) -> Vec<CapabilityItem> {
    let mut all = items.to_vec();
    internal_hooks::append_items(&mut all, settings);
    all
}

fn hook_manifests(
    items: &[CapabilityItem],
    settings: &Settings,
) -> HashMap<String, hook_sync::HookManifest> {
    let mut manifests = hook_sync::load_manifests(items);
    internal_hooks::insert_manifests(&mut manifests, settings);
    manifests
}

/// Inspect current per-tool state for every enabled tool.
pub fn inspect(items: &[CapabilityItem], settings: &Settings) -> InspectResult {
    let mut states: Vec<ToolCapabilityState> = Vec::new();
    let mut adapter_statuses: Vec<AdapterStatus> = Vec::new();
    let items = items_with_internal_hooks(items, settings);
    let manifests = hook_manifests(&items, settings);

    for tool in ToolId::ALL {
        let adapter = adapter_registry::resolve(settings, tool);
        match availability(&adapter) {
            Some(reason) => adapter_statuses.push(AdapterStatus {
                tool,
                available: false,
                unavailable_reason: Some(reason),
            }),
            None => {
                adapter_statuses.push(AdapterStatus {
                    tool,
                    available: true,
                    unavailable_reason: None,
                });
                states.extend(planner::inspect_tool(&items, &adapter));
                states.extend(inspect_hooks_for_adapter(&items, &manifests, &adapter));
            }
        }
    }

    InspectResult {
        states,
        adapter_statuses,
    }
}

/// `None` if the adapter is available; `Some(reason)` otherwise.
fn availability(adapter: &ResolvedAdapter) -> Option<String> {
    if !adapter.enabled {
        return Some("Tool is disabled in settings".to_string());
    }
    None
}

/// Build the operation plan for one tool against fresh disk state. Markdown
/// rules and hooks are excluded (handled by [`sync_rules`] / hook sync).
pub fn plan(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    desired_enabled: &HashMap<String, bool>,
    force: bool,
) -> Vec<PlannedOperation> {
    let adapter = adapter_registry::resolve(settings, tool);
    planner::build_plan(items, &adapter, desired_enabled, force)
}

/// Apply a batch of operations with a no-op progress sink. The Tauri shell uses
/// [`applier::apply`] directly so it can emit per-op progress events.
pub fn apply(ops: &[PlannedOperation]) -> ApplyResult {
    applier::apply(ops, |_, _, _, _| {})
}

/// Sync the markdown managed-rule block for one tool from the desired set.
/// No-op for tools without an instruction file (e.g. Cursor, whose rules are
/// symlinked and planned through [`plan`]).
pub fn sync_rules(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    desired_enabled: &HashMap<String, bool>,
) -> SyncRulesResult {
    let adapter = adapter_registry::resolve(settings, tool);
    let Some(instructions) = adapter.instructions_path.clone() else {
        return SyncRulesResult {
            outcome: RuleSyncOutcome::NoOp,
            errors: vec![],
        };
    };
    if adapter.projection_mode_for(CapabilityKind::Rule)
        != Some(ProjectionMode::MarkdownSectionSync)
    {
        return SyncRulesResult {
            outcome: RuleSyncOutcome::NoOp,
            errors: vec![],
        };
    }

    let enabled: Vec<&CapabilityItem> = items
        .iter()
        .filter(|it| {
            it.kind == CapabilityKind::Rule && desired_enabled.get(&it.id).copied().unwrap_or(false)
        })
        .collect();

    match rule_sync::sync_markdown_rules(&instructions, &enabled) {
        Ok(outcome) => SyncRulesResult {
            outcome,
            errors: vec![],
        },
        Err(e) => SyncRulesResult {
            outcome: RuleSyncOutcome::NoOp,
            errors: vec![e],
        },
    }
}

/// Sync the tool's hook config file (`json_section`) from the desired set. The
/// Not-Targeted sanitizer drops hooks whose effective targets exclude this tool
/// before the read-merge-write, so a stale toggle can never become a no-op edit.
pub fn sync_hooks(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    desired_enabled: &HashMap<String, bool>,
) -> SyncHooksResult {
    let adapter = adapter_registry::resolve(settings, tool);
    let items = items_with_internal_hooks(items, settings);
    let manifests = hook_manifests(&items, settings);

    let enabled: Vec<(&CapabilityItem, &hook_sync::HookManifest)> = items
        .iter()
        .filter(|it| it.kind == CapabilityKind::Hook)
        .filter(|it| {
            if internal_hooks::is_internal_item(it) {
                internal_hooks::item_enabled_for_tool(settings, it, tool)
            } else {
                desired_enabled.get(&it.id).copied().unwrap_or(false)
            }
        })
        .filter_map(|it| manifests.get(&it.id).map(|m| (it, m)))
        .filter(|(_, m)| m.effective_targets().contains(&tool))
        .collect();

    match sync_hooks_for_adapter(&adapter, &enabled) {
        Ok((outcome, notes)) => SyncHooksResult {
            outcome,
            notes,
            errors: vec![],
        },
        Err(e) => SyncHooksResult {
            outcome: HookSyncOutcome::NoOp,
            notes: vec![],
            errors: vec![e],
        },
    }
}

fn sync_hooks_for_adapter(
    adapter: &ResolvedAdapter,
    enabled: &[(&CapabilityItem, &hook_sync::HookManifest)],
) -> Result<(HookSyncOutcome, Vec<String>), crate::model::HookSyncError> {
    use crate::adapter_registry::ProjectionMode;
    use crate::model::CapabilityKind;

    match adapter.projection_mode_for(CapabilityKind::Hook) {
        Some(ProjectionMode::KiroHookFile) => {
            crate::kiro_hook_sync::sync_kiro_hooks(adapter, enabled)
        }
        Some(ProjectionMode::CopilotHookFile) => {
            crate::copilot_hook_sync::sync_copilot_hooks(adapter, enabled)
        }
        Some(ProjectionMode::JsonSection) => hook_sync::sync_json_hooks(adapter, enabled),
        _ => Ok((HookSyncOutcome::NoOp, vec![])),
    }
}

fn inspect_hooks_for_adapter(
    items: &[CapabilityItem],
    manifests: &HashMap<String, hook_sync::HookManifest>,
    adapter: &ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    use crate::adapter_registry::ProjectionMode;
    use crate::model::CapabilityKind;

    match adapter.projection_mode_for(CapabilityKind::Hook) {
        Some(ProjectionMode::KiroHookFile) => {
            crate::kiro_hook_sync::inspect_kiro_hooks(items, manifests, adapter)
        }
        Some(ProjectionMode::CopilotHookFile) => {
            crate::copilot_hook_sync::inspect_copilot_hooks(items, manifests, adapter)
        }
        Some(ProjectionMode::JsonSection) => hook_sync::inspect_hooks(items, manifests, adapter),
        _ => vec![],
    }
}

/// Item ids currently projected as enabled for one tool (links, managed copies,
/// markdown rules, hooks).
pub fn enabled_item_ids(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
) -> HashSet<String> {
    let adapter = adapter_registry::resolve(settings, tool);
    let items = items_with_internal_hooks(items, settings);
    let manifests = hook_manifests(&items, settings);
    let mut ids: HashSet<String> = planner::inspect_tool(&items, &adapter)
        .into_iter()
        .filter(|s| s.state == LinkState::Enabled)
        .map(|s| s.item_id)
        .collect();
    for s in inspect_hooks_for_adapter(&items, &manifests, &adapter) {
        if s.state == LinkState::Enabled {
            ids.insert(s.item_id);
        }
    }
    ids.retain(|id| {
        items
            .iter()
            .find(|it| it.id == *id)
            .map_or(true, |it| !internal_hooks::is_internal_item(it))
    });
    ids
}

/// Item ids matched by a suite's capability refs against the scanned items.
pub fn suite_matched_ids(items: &[CapabilityItem], suite: &SuiteDefinition) -> HashSet<String> {
    items
        .iter()
        .filter(|it| !internal_hooks::is_internal_item(it))
        .filter(|it| suite.capabilities.iter().any(|r| r.matches_item(it)))
        .map(|it| it.id.clone())
        .collect()
}

/// Enabled ids minus those matched by the effective suite, sorted.
pub fn manual_extras_from_enabled(
    enabled: &HashSet<String>,
    effective: &SuiteDefinition,
    items: &[CapabilityItem],
) -> Vec<String> {
    let matched = suite_matched_ids(items, effective);
    let mut extras: Vec<String> = enabled
        .iter()
        .filter(|id| !matched.contains(*id))
        .cloned()
        .collect();
    extras.sort();
    extras
}

/// Manual extras that would be lost when switching to `new_effective`.
/// Computes `enabled_now − prior_effective` (when a suite was already bound),
/// then keeps ids not covered by the new suite. First apply (no prior
/// effective) treats everything enabled outside the new suite as manual.
pub fn suite_apply_manual_extras(
    prior_effective: Option<&SuiteDefinition>,
    new_effective: &SuiteDefinition,
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
) -> Vec<String> {
    let enabled = enabled_item_ids(items, settings, tool);
    let new_matched = suite_matched_ids(items, new_effective);

    let live_manual: HashSet<String> = match prior_effective {
        None => manual_extras_from_enabled(&enabled, new_effective, items)
            .into_iter()
            .collect(),
        Some(prev) => manual_extras_from_enabled(&enabled, prev, items)
            .into_iter()
            .collect(),
    };

    let mut extras: Vec<String> = live_manual
        .into_iter()
        .filter(|id| !new_matched.contains(id) && items.iter().any(|it| it.id == *id))
        .collect();
    extras.sort();
    extras
}

/// Apply a suite to one tool as a full reset: every scanned item gets a desired
/// state (`true` iff a suite ref resolves to it, source-aware), then the
/// existing plan/apply + rule + hook sync pipeline runs. References whose source
/// is absent on this machine are counted as `skipped_absent_source` and
/// preserved (their projection cannot exist locally, so it is never deleted);
/// references whose source is present but match no scanned item are
/// `skipped_stale`. Reuses [`plan`]/[`apply`]/[`sync_rules`]/[`sync_hooks`] — no
/// parallel pipeline.
pub fn apply_suite(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    suite: &SuiteDefinition,
    manual_to_preserve: &[&str],
) -> ApplySuiteResult {
    let preserve: HashSet<&str> = manual_to_preserve.iter().copied().collect();
    let items = items_with_internal_hooks(items, settings);
    let desired: HashMap<String, bool> = items
        .iter()
        .map(|it| {
            let in_suite = !internal_hooks::is_internal_item(it)
                && suite.capabilities.iter().any(|r| r.matches_item(it));
            (it.id.clone(), in_suite || preserve.contains(it.id.as_str()))
        })
        .collect();

    let adapter = adapter_registry::resolve(settings, tool);
    // Suite apply is a non-destructive full reset; never take over real files.
    let ops = planner::build_plan(&items, &adapter, &desired, false);
    let apply_result = applier::apply(&ops, |_, _, _, _| {});
    let rule_sync = sync_rules(&items, settings, tool, &desired);
    let hook_sync = sync_hooks(&items, settings, tool, &desired);

    let local = settings.resolve_sources();
    let mut skipped_stale = 0u32;
    let mut skipped_absent_source = 0u32;
    for r in &suite.capabilities {
        if items
            .iter()
            .any(|it| !internal_hooks::is_internal_item(it) && r.matches_item(it))
        {
            continue;
        }
        match &r.source {
            // Qualified ref whose source is not here: preserve, don't delete.
            Some(src) if !source_present(&local, src) => skipped_absent_source += 1,
            // Present-source (or unqualified) ref with no matching item: stale.
            _ => skipped_stale += 1,
        }
    }

    let enabled_after = enabled_item_ids(&items, settings, tool);
    let manual_item_ids = manual_extras_from_enabled(&enabled_after, suite, &items);

    ApplySuiteResult {
        apply_result,
        rule_sync,
        hook_sync,
        skipped_stale,
        skipped_absent_source,
        suite: suite.clone(),
        manual_item_ids,
    }
}

/// Apply one suite to several tools as independent full resets. The testable
/// core of the suite<->tool binding re-sync: when a suite's capabilities
/// change, every bound tool is re-applied so its projection matches the new
/// set. Returns one [`ApplySuiteResult`] per tool, in the given order.
pub fn apply_suite_to_tools(
    items: &[CapabilityItem],
    settings: &Settings,
    suite: &SuiteDefinition,
    tools: &[ToolId],
    manual_by_tool: &HashMap<ToolId, Vec<String>>,
) -> Vec<ApplySuiteResult> {
    tools
        .iter()
        .map(|&tool| {
            let manual: Vec<&str> = manual_by_tool
                .get(&tool)
                .map(|ids| ids.iter().map(String::as_str).collect())
                .unwrap_or_default();
            apply_suite(items, settings, tool, suite, &manual)
        })
        .collect()
}

/// Build the effective suite to apply: `selected`'s capabilities unioned with
/// the base suite's, deduped by `(cap, source)`. The returned suite keeps
/// `selected`'s identity (id/name/is_base) so `ApplySuiteResult` and the
/// recorded binding still refer to the explicitly selected suite. A `None` base
/// (or base == selected) returns `selected` unchanged.
pub fn merge_base_caps(
    selected: &SuiteDefinition,
    base: Option<&SuiteDefinition>,
) -> SuiteDefinition {
    let mut merged = selected.clone();
    if let Some(base) = base {
        if base.id != selected.id {
            for r in &base.capabilities {
                if !merged.capabilities.contains(r) {
                    merged.capabilities.push(r.clone());
                }
            }
        }
    }
    merged
}

/// Resolve which suite owns each `(tool, item)` projection for the Manager to
/// lock. For every binding, an item matched by the bound suite's own refs is
/// owned by that suite; otherwise an item matched by the base suite's refs is
/// owned by the base (`from_base = true`). Bound-suite ownership wins when an
/// item is in both.
pub fn suite_ownership(
    items: &[CapabilityItem],
    bindings: &[SuiteBinding],
    suites: &[SuiteDefinition],
    base: Option<&SuiteDefinition>,
) -> Vec<SuiteOwnership> {
    let mut out = Vec::new();
    for b in bindings {
        let Some(selected) = suites.iter().find(|s| s.id == b.suite_id) else {
            continue;
        };
        for it in items {
            if internal_hooks::is_internal_item(it) {
                continue;
            }
            if selected.capabilities.iter().any(|r| r.matches_item(it)) {
                out.push(SuiteOwnership {
                    tool: b.tool_id,
                    item_id: it.id.clone(),
                    suite_id: selected.id.clone(),
                    suite_name: selected.name.clone(),
                    from_base: selected.is_base,
                });
            } else if let Some(base) = base.filter(|base| base.id != selected.id) {
                if base.capabilities.iter().any(|r| r.matches_item(it)) {
                    out.push(SuiteOwnership {
                        tool: b.tool_id,
                        item_id: it.id.clone(),
                        suite_id: base.id.clone(),
                        suite_name: base.name.clone(),
                        from_base: true,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SuiteCapabilityRef;
    use std::fs;
    use std::path::Path;

    #[test]
    fn merge_base_caps_unions_and_dedups_keeping_selected_identity() {
        let selected = suite("editor", &["skill:a", "skill:shared"]);
        let mut base = suite("base", &["skill:shared", "rule:global"]);
        base.id = "base-id".into();
        base.is_base = true;

        let merged = merge_base_caps(&selected, Some(&base));
        // Identity stays the selected suite's.
        assert_eq!(merged.id, selected.id);
        assert_eq!(merged.name, "editor");
        assert!(!merged.is_base);
        // Union, deduped: a, shared (once), global.
        let caps: Vec<&str> = merged.capabilities.iter().map(|r| r.cap.as_str()).collect();
        assert_eq!(caps, vec!["skill:a", "skill:shared", "rule:global"]);
    }

    #[test]
    fn merge_base_caps_is_noop_without_base_or_when_self_is_base() {
        let mut s = suite("base", &["skill:a"]);
        s.id = "x".into();
        assert_eq!(merge_base_caps(&s, None).capabilities.len(), 1);
        // base == selected (same id) must not double-add.
        assert_eq!(merge_base_caps(&s, Some(&s)).capabilities.len(), 1);
    }

    #[test]
    fn suite_ownership_attributes_base_and_bound_items() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        write(&root.path().join("skills/g/SKILL.md"), "# g");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        let mut editor = suite("editor", &["skill:a"]);
        editor.id = "ed".into();
        let mut base = suite("globals", &["skill:g"]);
        base.id = "ba".into();
        base.is_base = true;
        let suites = vec![editor, base.clone()];
        let bindings = vec![SuiteBinding {
            tool_id: ToolId::Codex,
            suite_id: "ed".into(),
            manual_item_ids: vec![],
        }];

        let own = suite_ownership(&scanned.items, &bindings, &suites, Some(&base));
        let a = own.iter().find(|o| o.item_id == "skill:a").unwrap();
        assert_eq!(a.suite_id, "ed");
        assert!(!a.from_base, "bound suite owns skill:a");
        let g = own.iter().find(|o| o.item_id == "skill:g").unwrap();
        assert_eq!(g.suite_id, "ba");
        assert!(g.from_base, "base owns skill:g");
    }

    #[test]
    fn suite_ownership_prefers_bound_suite_when_item_in_both() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/shared/SKILL.md"), "# s");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        let mut editor = suite("editor", &["skill:shared"]);
        editor.id = "ed".into();
        let mut base = suite("globals", &["skill:shared"]);
        base.id = "ba".into();
        base.is_base = true;
        let suites = vec![editor, base.clone()];
        let bindings = vec![SuiteBinding {
            tool_id: ToolId::Codex,
            suite_id: "ed".into(),
            manual_item_ids: vec![],
        }];

        let own = suite_ownership(&scanned.items, &bindings, &suites, Some(&base));
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].suite_id, "ed", "bound suite wins over base");
        assert!(!own[0].from_base);
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn scan_uses_resolved_sources() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("skills/a/SKILL.md"), "# a");
        let mut settings = Settings {
            shared_root: dir.path().to_path_buf(),
            ..Settings::default()
        };
        // This test targets source resolution, not usage tracing; disable
        // tracing so its default-on virtual hook items don't inflate the count.
        settings.usage_tracing.enabled = false;
        let result = scan(&settings);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "skill:a");
        // Carries the synthesized Default source identity.
        assert_eq!(result.items[0].source_label, "Default");
    }

    #[test]
    fn apply_suite_full_reset_enables_suite_and_disables_rest() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# keep");
        write(&root.path().join("skills/drop/SKILL.md"), "# drop");

        // Sandbox all tool paths so the full-reset's rule/hook syncs never touch
        // the real home directory.
        let settings = Settings::sandboxed(root.path(), tools.path());

        let scanned = scan(&settings);
        // Pre-enable both by applying a suite that contains both.
        let both = SuiteDefinition {
            id: "1".into(),
            name: "both".into(),
            description: None,
            capabilities: vec!["skill:keep".into(), "skill:drop".into()],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        apply_suite(&scanned.items, &settings, ToolId::Codex, &both, &[]);
        let before = inspect(&scanned.items, &settings);
        assert_eq!(
            before
                .states
                .iter()
                .filter(|s| s.tool == ToolId::Codex && s.state == crate::model::LinkState::Enabled)
                .count(),
            2
        );

        // Now apply a suite with only `keep`: full reset disables `drop`.
        let only_keep = SuiteDefinition {
            capabilities: vec!["skill:keep".into(), "skill:gone".into()],
            ..both
        };
        let result = apply_suite(&scanned.items, &settings, ToolId::Codex, &only_keep, &[]);
        assert_eq!(result.skipped_stale, 1, "skill:gone not in scan");

        let after = inspect(&scanned.items, &settings);
        let enabled: Vec<&str> = after
            .states
            .iter()
            .filter(|s| s.tool == ToolId::Codex && s.state == crate::model::LinkState::Enabled)
            .map(|s| s.item_id.as_str())
            .collect();
        assert_eq!(enabled, vec!["skill:keep"]);
    }

    #[test]
    fn apply_suite_surfaces_managed_copy_apply_errors() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/blocked/SKILL.md"), "# blocked");
        let blocker = tools.path().join("not-a-directory");
        write(&blocker, "file");
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        settings.tools.claude.skills_path = blocker.join("skills");
        let scanned = scan(&settings);

        let result = apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &suite("blocked", &["skill:blocked"]),
            &[],
        );

        assert_eq!(result.apply_result.errors.len(), 1);
        assert!(result.rule_sync.errors.is_empty());
        assert!(result.hook_sync.errors.is_empty());
    }

    #[test]
    fn apply_suite_surfaces_rule_and_hook_sync_errors() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("rules/team.md"), "# rule");
        write(
            &root.path().join("hooks/fmt/hook.json"),
            r#"{ "id": "fmt", "command": "run", "events": [{"name":"Stop"}] }"#,
        );
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        let instructions = tools.path().join("codex-instructions");
        fs::create_dir_all(&instructions).unwrap();
        settings.tools.codex.instructions_path = Some(instructions);
        settings.tools.cursor.hooks_enabled = true;
        let hooks_file = tools.path().join("cursor-hooks");
        fs::create_dir_all(&hooks_file).unwrap();
        settings.tools.cursor.hooks_file = Some(hooks_file);
        let scanned = scan(&settings);

        let rule_result = apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("rule", &["rule:team.md"]),
            &[],
        );
        let hook_result = apply_suite(
            &scanned.items,
            &settings,
            ToolId::Cursor,
            &suite("hook", &["hook:fmt"]),
            &[],
        );

        assert_eq!(rule_result.rule_sync.errors.len(), 1);
        assert_eq!(hook_result.hook_sync.errors.len(), 1);
    }

    /// Item ids currently `Enabled` for one tool, sorted for stable asserts.
    fn enabled_ids(settings: &Settings, items: &[CapabilityItem], tool: ToolId) -> Vec<String> {
        let mut ids: Vec<String> = inspect(items, settings)
            .states
            .into_iter()
            .filter(|s| s.tool == tool && s.state == crate::model::LinkState::Enabled)
            .map(|s| s.item_id)
            .collect();
        ids.sort();
        ids
    }

    fn suite(name: &str, caps: &[&str]) -> SuiteDefinition {
        SuiteDefinition {
            id: name.into(),
            name: name.into(),
            description: None,
            capabilities: caps.iter().map(|s| (*s).into()).collect(),
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn enable_usage_tracing(mut settings: Settings, tools: Vec<ToolId>) -> Settings {
        settings.usage_tracing.enabled = true;
        settings.usage_tracing.capture_tools = tools;
        settings.usage_tracing.collector_token = "test-token".into();
        settings
    }

    fn hook_file(settings: &Settings, tool: ToolId) -> String {
        let path = match tool {
            ToolId::Cursor => settings.tools.cursor.hooks_file.as_ref(),
            ToolId::Claude => settings.tools.claude.hooks_file.as_ref(),
            ToolId::Codex => settings.tools.codex.hooks_file.as_ref(),
            _ => None,
        }
        .unwrap();
        fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn scan_includes_settings_managed_usage_tracer_hooks() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = enable_usage_tracing(
            Settings::sandboxed(root.path(), tools.path()),
            vec![ToolId::Cursor],
        );

        let scanned = scan(&settings);
        let tracer = scanned
            .items
            .iter()
            .find(|it| it.id == "hook:agentic-hub-usage-tracer-cursor")
            .expect("virtual tracer hook should be visible");

        assert_eq!(tracer.source_id, "agentic-hub");
        assert_eq!(tracer.source_label, "Agentic Hub");
        assert_eq!(tracer.kind, CapabilityKind::Hook);
    }

    #[test]
    fn inspect_reports_settings_managed_usage_tracer_disabled_when_config_off() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        settings.usage_tracing.enabled = true;
        settings.usage_tracing.capture_tools = vec![ToolId::Claude];
        let scanned = scan(&settings);

        let result = inspect(&scanned.items, &settings);

        assert!(scanned
            .items
            .iter()
            .any(|it| it.id == "hook:agentic-hub-usage-tracer-cursor"));
        assert!(result.states.iter().any(|s| {
            s.tool == ToolId::Cursor
                && s.item_id == "hook:agentic-hub-usage-tracer-cursor"
                && s.state == LinkState::Disabled
        }));
    }

    #[test]
    fn sync_hooks_preserves_settings_managed_usage_tracer() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(
            &root.path().join("hooks/fmt/hook.json"),
            r#"{ "id": "fmt", "command": "run", "events": [{"name":"Stop"}] }"#,
        );
        let settings = enable_usage_tracing(
            Settings::sandboxed(root.path(), tools.path()),
            vec![ToolId::Cursor],
        );
        let scanned = scan(&settings);

        let mut desired = HashMap::new();
        desired.insert("hook:fmt".to_string(), true);
        sync_hooks(&scanned.items, &settings, ToolId::Cursor, &desired);

        let written = hook_file(&settings, ToolId::Cursor);
        assert!(written.contains("\"hookId\": \"fmt\""));
        assert!(written.contains("\"hookId\": \"agentic-hub-usage-tracer-cursor\""));
    }

    #[test]
    fn apply_suite_removes_dropped_managed_copy_on_reapply() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        // Claude skills are managed copies (FileSync) — a real file lands on disk.
        write(&root.path().join("skills/keep/SKILL.md"), "# keep");
        write(&root.path().join("skills/drop/SKILL.md"), "# drop");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &suite("both", &["skill:keep", "skill:drop"]),
            &[],
        );
        let copy = settings.tools.claude.skills_path.join("drop");
        assert!(copy.exists(), "drop projected before re-apply");
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude),
            vec!["skill:drop", "skill:keep"]
        );

        // Re-apply a suite without `drop`: the full reset removes its copy.
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &suite("only-keep", &["skill:keep"]),
            &[],
        );
        assert!(!copy.exists(), "drop's managed copy cleaned on re-apply");
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude),
            vec!["skill:keep"]
        );
    }

    #[test]
    fn apply_suite_rewrites_rules_block_to_exactly_the_suite_set() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("rules/a.mdc"), "rule A body");
        write(&root.path().join("rules/b.mdc"), "rule B body");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let instr = settings.tools.codex.instructions_path.clone().unwrap();
        fs::create_dir_all(instr.parent().unwrap()).unwrap();
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("a-only", &["rule:a.mdc"]),
            &[],
        );
        let written = fs::read_to_string(&instr).unwrap();
        assert!(written.contains("### a.mdc"), "suite rule listed");
        assert!(!written.contains("### b.mdc"), "non-suite rule absent");

        // Re-apply with the other rule: the block flips to exactly {b}.
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("b-only", &["rule:b.mdc"]),
            &[],
        );
        let written = fs::read_to_string(&instr).unwrap();
        assert!(written.contains("### b.mdc"), "new suite rule listed");
        assert!(!written.contains("### a.mdc"), "dropped rule removed");
    }

    #[test]
    fn apply_suite_commands_reflect_exactly_the_suite_set() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(
            &root.path().join("commands/review/code-review.md"),
            "# review",
        );
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Cursor,
            &suite("with-cmd", &["command:review/code-review.md"]),
            &[],
        );
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Cursor),
            vec!["command:review/code-review.md"],
            "command projected when in the suite"
        );
        let target = settings
            .tools
            .cursor
            .commands_path
            .as_ref()
            .unwrap()
            .join("review/code-review.md");
        assert!(target.exists(), "command symlink/copy exists on disk");

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Cursor,
            &suite("empty", &[]),
            &[],
        );
        assert!(
            enabled_ids(&settings, &scanned.items, ToolId::Cursor).is_empty(),
            "command cleared by the full reset"
        );
        assert!(!target.exists(), "command projection removed on re-apply");
    }

    #[test]
    fn apply_suite_hooks_reflect_exactly_the_suite_set() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        // Default-target hook (Cursor/Claude/Codex) with a Claude-supported event.
        write(
            &root.path().join("hooks/fmt/hook.json"),
            r#"{ "id": "fmt", "command": "run", "events": [{"name":"Stop"}] }"#,
        );
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &suite("with-hook", &["hook:fmt"]),
            &[],
        );
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude),
            vec!["hook:fmt"],
            "hook projected when in the suite"
        );

        // Re-apply an empty suite: the hook is cleared from the tool's config.
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &suite("empty", &[]),
            &[],
        );
        assert!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude).is_empty(),
            "hook cleared by the full reset"
        );
    }

    #[test]
    fn apply_suite_empty_keeps_settings_managed_usage_tracer() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(
            &root.path().join("hooks/fmt/hook.json"),
            r#"{ "id": "fmt", "command": "run", "events": [{"name":"Stop"}] }"#,
        );
        let settings = enable_usage_tracing(
            Settings::sandboxed(root.path(), tools.path()),
            vec![ToolId::Cursor],
        );
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Cursor,
            &suite("with-hook", &["hook:fmt"]),
            &[],
        );
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Cursor,
            &suite("empty", &[]),
            &[],
        );

        let written = hook_file(&settings, ToolId::Cursor);
        assert!(!written.contains("\"hookId\": \"fmt\""));
        assert!(written.contains("\"hookId\": \"agentic-hub-usage-tracer-cursor\""));
    }

    #[test]
    fn apply_suite_routes_opt_in_tool_skills_and_hooks() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/review/SKILL.md"), "# review");
        write(
            &root.path().join("hooks/fmt/hook.json"),
            r#"{
                "id": "fmt",
                "command": "run",
                "events": [{"name":"Stop"}],
                "targets": ["kiro", "copilot", "antigravity"]
            }"#,
        );
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);
        let selected = suite("opt-in", &["skill:review", "hook:fmt"]);

        for tool in [ToolId::Kiro, ToolId::Copilot, ToolId::Antigravity] {
            apply_suite(&scanned.items, &settings, tool, &selected, &[]);
            let enabled = enabled_ids(&settings, &scanned.items, tool);
            assert!(
                enabled.iter().any(|id| id == "skill:review"),
                "{tool:?}: {enabled:?}"
            );
            assert!(
                enabled.iter().any(|id| id == "hook:fmt"),
                "{tool:?}: {enabled:?}"
            );
        }

        assert!(settings.tools.kiro.skills_path.join("review").exists());
        assert!(settings
            .tools
            .kiro
            .hooks_dir
            .as_ref()
            .unwrap()
            .join("fmt.json")
            .exists());
        assert!(settings.tools.copilot.skills_path.join("review").exists());
        assert!(settings
            .tools
            .copilot
            .hooks_dir
            .as_ref()
            .unwrap()
            .join("fmt.json")
            .exists());
        assert!(settings
            .tools
            .antigravity
            .skills_path
            .join("review")
            .exists());
        assert!(settings
            .tools
            .antigravity
            .hooks_file
            .as_ref()
            .unwrap()
            .exists());
    }

    #[test]
    fn apply_empty_suite_disables_everything() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        write(&root.path().join("skills/b/SKILL.md"), "# b");
        write(&root.path().join("rules/r.mdc"), "rule body");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let instr = settings.tools.codex.instructions_path.clone().unwrap();
        fs::create_dir_all(instr.parent().unwrap()).unwrap();
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("all", &["skill:a", "skill:b", "rule:r.mdc"]),
            &[],
        );
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex).len(),
            3
        );

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("empty", &[]),
            &[],
        );
        assert!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex).is_empty(),
            "empty suite disables every capability"
        );
        assert!(
            !fs::read_to_string(&instr)
                .unwrap_or_default()
                .contains("### r.mdc"),
            "rule block cleared too"
        );
    }

    #[test]
    fn apply_suite_refreshes_stale_managed_copy() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# v1");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        let only = suite("keep", &["skill:keep"]);
        apply_suite(&scanned.items, &settings, ToolId::Claude, &only, &[]);
        let copy = settings.tools.claude.skills_path.join("keep/SKILL.md");
        assert_eq!(fs::read_to_string(&copy).unwrap(), "# v1");

        // Edit the source, then re-apply the same suite: the copy refreshes.
        write(&root.path().join("skills/keep/SKILL.md"), "# v2 fresh");
        let rescanned = scan(&settings);
        let result = apply_suite(&rescanned.items, &settings, ToolId::Claude, &only, &[]);
        assert!(result.apply_result.errors.is_empty());
        assert_eq!(
            fs::read_to_string(&copy).unwrap(),
            "# v2 fresh",
            "stale managed copy refreshed from source"
        );
    }

    #[test]
    fn apply_suite_to_tools_applies_each_tool_independently() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        let results = apply_suite_to_tools(
            &scanned.items,
            &settings,
            &suite("s", &["skill:a"]),
            &[ToolId::Codex, ToolId::Claude],
            &HashMap::new(),
        );
        assert_eq!(results.len(), 2);
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex),
            vec!["skill:a"]
        );
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude),
            vec!["skill:a"]
        );
    }

    #[test]
    fn apply_suite_skips_ref_whose_source_is_absent_and_preserves() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        // A synced ref qualified to a source that does not exist on this machine.
        let absent = SuiteCapabilityRef {
            cap: "skill:a".into(),
            source: Some(crate::model::SourceRef {
                rel_home: "~/definitely-not-here".into(),
                folder: "definitely-not-here".into(),
            }),
        };
        let suite = SuiteDefinition {
            id: "s".into(),
            name: "s".into(),
            description: None,
            capabilities: vec![absent],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let result = apply_suite(&scanned.items, &settings, ToolId::Codex, &suite, &[]);
        assert_eq!(result.skipped_absent_source, 1, "source not present here");
        assert_eq!(result.skipped_stale, 0);
        // skill:a exists locally but under a different source, so the absent-source
        // ref never mis-resolves onto it.
        assert!(enabled_ids(&settings, &scanned.items, ToolId::Codex).is_empty());
    }

    #[test]
    fn apply_suite_matches_qualified_present_source_and_legacy_bare_ref() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);
        let present = scanned.items[0].source.clone();

        // Qualified to the present source -> matches and enables.
        let qualified = SuiteDefinition {
            id: "q".into(),
            name: "q".into(),
            description: None,
            capabilities: vec![SuiteCapabilityRef {
                cap: "skill:a".into(),
                source: Some(present),
            }],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let r = apply_suite(&scanned.items, &settings, ToolId::Codex, &qualified, &[]);
        assert_eq!(r.skipped_absent_source, 0);
        assert_eq!(r.skipped_stale, 0);
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex),
            vec!["skill:a"]
        );

        // A legacy bare ref (source: None) still matches by id alone.
        let bare = suite("b", &["skill:a"]);
        let r2 = apply_suite(&scanned.items, &settings, ToolId::Codex, &bare, &[]);
        assert_eq!(r2.skipped_stale, 0);
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex),
            vec!["skill:a"]
        );
    }

    #[test]
    fn apply_suite_does_not_misresolve_same_id_across_present_sources() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&a.path().join("skills/dup/SKILL.md"), "# a-dup");
        write(&b.path().join("skills/dup/SKILL.md"), "# b-dup");
        let mut settings = Settings::sandboxed(a.path(), tools.path());
        settings.sources = vec![
            crate::settings::SourceConfig {
                id: String::new(),
                label: "A".into(),
                path: a.path().to_path_buf(),
            },
            crate::settings::SourceConfig {
                id: String::new(),
                label: "B".into(),
                path: b.path().to_path_buf(),
            },
        ];
        let scanned = scan(&settings);
        // First-source-wins: only A contributes the scanned skill:dup.
        let a_ref = scanned
            .items
            .iter()
            .find(|i| i.id == "skill:dup")
            .unwrap()
            .source
            .clone();
        // B is present too, but it is NOT the winner for skill:dup.
        let b_ref = crate::settings::SourceConfig {
            id: String::new(),
            label: "B".into(),
            path: b.path().to_path_buf(),
        }
        .portable_ref();
        assert!(!a_ref.matches(&b_ref), "two tempdirs are distinct sources");

        let suite = SuiteDefinition {
            id: "m".into(),
            name: "m".into(),
            description: None,
            capabilities: vec![SuiteCapabilityRef {
                cap: "skill:dup".into(),
                source: Some(b_ref),
            }],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let r = apply_suite(&scanned.items, &settings, ToolId::Codex, &suite, &[]);
        // B is present but its skill:dup is shadowed, so nothing resolves: stale,
        // not absent, and A's skill:dup is never mis-enabled.
        assert!(enabled_ids(&settings, &scanned.items, ToolId::Codex).is_empty());
        assert_eq!(r.skipped_absent_source, 0);
        assert_eq!(r.skipped_stale, 1);
    }

    #[test]
    fn suite_apply_manual_extras_ignores_old_suite_items() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# keep");
        write(&root.path().join("skills/extra/SKILL.md"), "# extra");
        write(&root.path().join("skills/new/SKILL.md"), "# new");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("keep-only", &["skill:keep"]),
            &[],
        );
        // User manually enabled `extra` via the Manager (disk updated; binding still empty).
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("keep-only", &["skill:keep"]),
            &["skill:extra"],
        );
        let prior_effective = suite("keep-only", &["skill:keep"]);
        let new_suite = suite("new-only", &["skill:new"]);
        assert_eq!(
            suite_apply_manual_extras(
                Some(&prior_effective),
                &new_suite,
                &scanned.items,
                &settings,
                ToolId::Codex,
            ),
            vec!["skill:extra"]
        );
    }

    #[test]
    fn suite_apply_manual_extras_finds_manager_additions_with_empty_binding_manual_ids() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/a/SKILL.md"), "# a");
        write(&root.path().join("skills/x1/SKILL.md"), "# x1");
        write(&root.path().join("skills/x2/SKILL.md"), "# x2");
        write(&root.path().join("skills/b/SKILL.md"), "# b");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("suite-a", &["skill:a"]),
            &[],
        );
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("suite-a", &["skill:a"]),
            &["skill:x1", "skill:x2"],
        );
        let prior = suite("suite-a", &["skill:a"]);
        let next = suite("suite-b", &["skill:b"]);
        assert_eq!(
            suite_apply_manual_extras(
                Some(&prior),
                &next,
                &scanned.items,
                &settings,
                ToolId::Codex,
            ),
            vec!["skill:x1", "skill:x2"]
        );
    }

    #[test]
    fn apply_suite_manual_preserve_keeps_only_manual_extras() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&root.path().join("skills/keep/SKILL.md"), "# keep");
        write(&root.path().join("skills/extra/SKILL.md"), "# extra");
        write(&root.path().join("skills/new/SKILL.md"), "# new");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &suite("keep-only", &["skill:keep"]),
            &["skill:extra"],
        );
        let new_suite = suite("new-only", &["skill:new"]);
        let result = apply_suite(
            &scanned.items,
            &settings,
            ToolId::Codex,
            &new_suite,
            &["skill:extra"],
        );
        assert_eq!(result.manual_item_ids, vec!["skill:extra"]);
        let mut enabled = enabled_ids(&settings, &scanned.items, ToolId::Codex);
        enabled.sort();
        assert_eq!(enabled, vec!["skill:extra", "skill:new"]);

        apply_suite(&scanned.items, &settings, ToolId::Codex, &new_suite, &[]);
        assert_eq!(
            enabled_ids(&settings, &scanned.items, ToolId::Codex),
            vec!["skill:new"]
        );
    }

    #[test]
    fn inspect_reports_all_tools_and_skips_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&dir.path().join("skills/a/SKILL.md"), "# a");
        let mut settings = Settings::sandboxed(dir.path(), tools.path());
        settings.tools.openclaw.enabled = false;
        settings.tools.kiro.enabled = false;
        settings.tools.copilot.enabled = false;
        settings.tools.antigravity.enabled = false;

        let scanned = scan(&settings);
        let result = inspect(&scanned.items, &settings);

        // One status per tool.
        assert_eq!(result.adapter_statuses.len(), 8);
        let openclaw = result
            .adapter_statuses
            .iter()
            .find(|s| s.tool == ToolId::Openclaw)
            .unwrap();
        assert!(!openclaw.available);
        assert!(openclaw.unavailable_reason.is_some());
        let kiro = result
            .adapter_statuses
            .iter()
            .find(|s| s.tool == ToolId::Kiro)
            .unwrap();
        assert!(!kiro.available);

        // Disabled tool produces no states; enabled tools each inspect the skill.
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Openclaw));
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Kiro));
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Copilot));
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Antigravity));
        assert!(result
            .states
            .iter()
            .any(|s| s.tool == ToolId::Codex && s.item_id == "skill:a"));
    }
}
