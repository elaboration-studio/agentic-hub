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
use crate::model::{
    ApplyResult, ApplySuiteResult, CapabilityItem, CapabilityKind, HookSyncOutcome,
    PlannedOperation, RuleSyncOutcome, ScanResult, SuiteDefinition, SyncHooksResult,
    SyncRulesResult, ToolCapabilityState, ToolId,
};
use crate::planner;
use crate::rule_sync;
use crate::scanner;
use crate::settings::Settings;

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
/// root fallback). Hook items are annotated with `hook.json` validation results.
pub fn scan(settings: &Settings) -> ScanResult {
    let mut result = scanner::scan_all(&settings.resolve_sources());
    hook_sync::annotate_validation(&mut result.items);
    result
}

/// Inspect current per-tool state for every enabled tool.
pub fn inspect(items: &[CapabilityItem], settings: &Settings) -> InspectResult {
    let mut states: Vec<ToolCapabilityState> = Vec::new();
    let mut adapter_statuses: Vec<AdapterStatus> = Vec::new();
    let manifests = hook_sync::load_manifests(items);

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
                states.extend(planner::inspect_tool(items, &adapter));
                states.extend(hook_sync::inspect_hooks(items, &manifests, &adapter));
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
    let manifests = hook_sync::load_manifests(items);

    let enabled: Vec<(&CapabilityItem, &hook_sync::HookManifest)> = items
        .iter()
        .filter(|it| it.kind == CapabilityKind::Hook)
        .filter(|it| desired_enabled.get(&it.id).copied().unwrap_or(false))
        .filter_map(|it| manifests.get(&it.id).map(|m| (it, m)))
        .filter(|(_, m)| m.effective_targets().contains(&tool))
        .collect();

    match hook_sync::sync_json_hooks(&adapter, &enabled) {
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

/// Apply a suite to one tool as a full reset: every scanned item gets a desired
/// state (`true` iff in the suite), then the existing plan/apply + rule + hook
/// sync pipeline runs. Capability IDs not provided by any source are counted as
/// skipped-stale. Reuses [`plan`]/[`apply`]/[`sync_rules`]/[`sync_hooks`] — no
/// parallel pipeline.
pub fn apply_suite(
    items: &[CapabilityItem],
    settings: &Settings,
    tool: ToolId,
    suite: &SuiteDefinition,
) -> ApplySuiteResult {
    let suite_set: HashSet<&str> = suite.capabilities.iter().map(String::as_str).collect();
    let desired: HashMap<String, bool> = items
        .iter()
        .map(|it| (it.id.clone(), suite_set.contains(it.id.as_str())))
        .collect();

    let adapter = adapter_registry::resolve(settings, tool);
    // Suite apply is a non-destructive full reset; never take over real files.
    let ops = planner::build_plan(items, &adapter, &desired, false);
    let apply_result = applier::apply(&ops, |_, _, _, _| {});
    let _ = sync_rules(items, settings, tool, &desired);
    let _ = sync_hooks(items, settings, tool, &desired);

    let skipped_stale = suite
        .capabilities
        .iter()
        .filter(|cap| !items.iter().any(|i| &i.id == *cap))
        .count() as u32;

    ApplySuiteResult {
        apply_result,
        skipped_stale,
        suite: suite.clone(),
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
) -> Vec<ApplySuiteResult> {
    tools
        .iter()
        .map(|&tool| apply_suite(items, settings, tool, suite))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

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
        let settings = Settings {
            shared_root: dir.path().to_path_buf(),
            ..Settings::default()
        };
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
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        apply_suite(&scanned.items, &settings, ToolId::Codex, &both);
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
        let result = apply_suite(&scanned.items, &settings, ToolId::Codex, &only_keep);
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
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            created_at: "t".into(),
            updated_at: "t".into(),
        }
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
        );
        let written = fs::read_to_string(&instr).unwrap();
        assert!(written.contains("### b.mdc"), "new suite rule listed");
        assert!(!written.contains("### a.mdc"), "dropped rule removed");
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
        );
        assert!(
            enabled_ids(&settings, &scanned.items, ToolId::Claude).is_empty(),
            "hook cleared by the full reset"
        );
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
        apply_suite(&scanned.items, &settings, ToolId::Claude, &only);
        let copy = settings.tools.claude.skills_path.join("keep/SKILL.md");
        assert_eq!(fs::read_to_string(&copy).unwrap(), "# v1");

        // Edit the source, then re-apply the same suite: the copy refreshes.
        write(&root.path().join("skills/keep/SKILL.md"), "# v2 fresh");
        let rescanned = scan(&settings);
        let result = apply_suite(&rescanned.items, &settings, ToolId::Claude, &only);
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
    fn inspect_reports_all_tools_and_skips_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(&dir.path().join("skills/a/SKILL.md"), "# a");
        let mut settings = Settings::sandboxed(dir.path(), tools.path());
        settings.tools.openclaw.enabled = false;

        let scanned = scan(&settings);
        let result = inspect(&scanned.items, &settings);

        // One status per tool.
        assert_eq!(result.adapter_statuses.len(), 4);
        let openclaw = result
            .adapter_statuses
            .iter()
            .find(|s| s.tool == ToolId::Openclaw)
            .unwrap();
        assert!(!openclaw.available);
        assert!(openclaw.unavailable_reason.is_some());

        // Disabled tool produces no states; enabled tools each inspect the skill.
        assert!(!result.states.iter().any(|s| s.tool == ToolId::Openclaw));
        assert!(result
            .states
            .iter()
            .any(|s| s.tool == ToolId::Codex && s.item_id == "skill:a"));
    }
}
