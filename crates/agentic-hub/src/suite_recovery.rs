use agentic_core::api;
use agentic_core::model::{ApplySuiteResult, ToolId};
use agentic_core::settings::Settings;
use agentic_core::suite_binding_store::SuiteBindingStore;
use agentic_core::suite_store::SuiteStore;

use crate::error::IpcError;
use crate::watcher;

pub fn resync_suite_binding_for_tool(
    settings: &Settings,
    suites: &SuiteStore,
    bindings: &SuiteBindingStore,
    tool: ToolId,
) -> Result<ApplySuiteResult, IpcError> {
    watcher::with_projection_transaction(|| {
        let binding = bindings.get(tool)?.ok_or_else(|| {
            IpcError::new(
                "suite_binding_not_found",
                "This tool no longer has a live suite binding",
            )
        })?;
        let mut selected = suites.get(&binding.suite_id)?.ok_or_else(|| {
            IpcError::new(
                "suite_not_found",
                "The suite selected by this tool binding no longer exists",
            )
        })?;
        let scanned = api::scan(settings);
        SuiteStore::backfill_sources(&mut selected, &scanned.items);
        let base = suites.base()?;
        let result = agentic_core::resync_suite_binding(
            &scanned.items,
            settings,
            &binding,
            &selected,
            base.as_ref(),
        );
        if result.is_full_success() {
            bindings.record(tool, &binding.suite_id, result.manual_item_ids.clone())?;
        }
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use agentic_core::model::ToolId;
    use agentic_core::settings::Settings;
    use agentic_core::suite_binding_store::SuiteBindingStore;
    use agentic_core::suite_store::{SuiteCreateInput, SuiteStore};

    use super::resync_suite_binding_for_tool;

    fn stores() -> (tempfile::TempDir, SuiteStore, SuiteBindingStore) {
        let dir = tempfile::tempdir().unwrap();
        let suites = SuiteStore::with_path(dir.path().join("suites.json"));
        let bindings = SuiteBindingStore::with_path(dir.path().join("bindings.json"));
        (dir, suites, bindings)
    }

    fn sandboxed_settings(root: &std::path::Path, tools: &std::path::Path) -> Settings {
        let mut settings = Settings::default();
        settings.sources.clear();
        settings.shared_root = root.to_path_buf();
        settings.usage_tracing.enabled = false;
        let claude = &mut settings.tools.claude;
        claude.enabled = true;
        claude.skills_path = tools.join("claude/skills");
        claude.agents_path = tools.join("claude/agents");
        claude.rules_path = tools.join("claude/rules");
        claude.instructions_path = Some(tools.join("claude/CLAUDE.md"));
        claude.hooks_enabled = false;
        claude.hooks_file = Some(tools.join("claude/hooks.json"));
        claude.commands_path = Some(tools.join("claude/commands"));
        settings
    }

    #[test]
    fn resync_rejects_tool_without_live_binding() {
        let (_dir, suites, bindings) = stores();
        let settings = Settings::default();

        let error = resync_suite_binding_for_tool(&settings, &suites, &bindings, ToolId::Claude)
            .unwrap_err();

        assert_eq!(error.code, "suite_binding_not_found");
    }

    #[test]
    fn resync_rejects_binding_whose_selected_suite_is_missing() {
        let (_dir, suites, bindings) = stores();
        bindings
            .record(ToolId::Claude, "gone", vec!["skill:manual".into()])
            .unwrap();
        let settings = Settings::default();

        let error = resync_suite_binding_for_tool(&settings, &suites, &bindings, ToolId::Claude)
            .unwrap_err();

        assert_eq!(error.code, "suite_not_found");
        assert_eq!(
            bindings.get(ToolId::Claude).unwrap().unwrap().suite_id,
            "gone"
        );
    }

    #[test]
    fn resync_keeps_selected_binding_and_manual_extras() {
        let (_dir, suites, bindings) = stores();
        let selected = suites
            .create(SuiteCreateInput {
                name: "Selected".into(),
                description: None,
                capabilities: vec!["skill:selected".into()],
                agent: None,
            })
            .unwrap();
        let base = suites
            .create(SuiteCreateInput {
                name: "Base".into(),
                description: None,
                capabilities: vec!["skill:base".into()],
                agent: None,
            })
            .unwrap();
        suites.set_base(Some(&base.id)).unwrap();
        bindings
            .record(ToolId::Claude, &selected.id, vec!["skill:manual".into()])
            .unwrap();

        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("skills/selected")).unwrap();
        std::fs::create_dir_all(root.path().join("skills/base")).unwrap();
        std::fs::create_dir_all(root.path().join("skills/manual")).unwrap();
        std::fs::write(root.path().join("skills/selected/SKILL.md"), "selected").unwrap();
        std::fs::write(root.path().join("skills/base/SKILL.md"), "base").unwrap();
        std::fs::write(root.path().join("skills/manual/SKILL.md"), "manual").unwrap();
        let settings = sandboxed_settings(root.path(), tools.path());
        let result =
            resync_suite_binding_for_tool(&settings, &suites, &bindings, ToolId::Claude).unwrap();

        assert_eq!(result.suite.id, selected.id);
        assert_eq!(result.manual_item_ids, vec!["skill:manual"]);
        let persisted = bindings.get(ToolId::Claude).unwrap().unwrap();
        assert_eq!(persisted.suite_id, selected.id);
        assert_eq!(persisted.manual_item_ids, vec!["skill:manual"]);
    }

    #[test]
    fn resync_apply_failure_keeps_persisted_manual_extras_unchanged() {
        let (_dir, suites, bindings) = stores();
        let selected = suites
            .create(SuiteCreateInput {
                name: "Selected".into(),
                description: None,
                capabilities: vec!["skill:selected".into()],
                agent: None,
            })
            .unwrap();
        bindings
            .record(ToolId::Claude, &selected.id, vec!["skill:recorded".into()])
            .unwrap();

        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("skills/selected")).unwrap();
        std::fs::write(root.path().join("skills/selected/SKILL.md"), "selected").unwrap();
        let blocker = tools.path().join("not-a-directory");
        std::fs::write(&blocker, "file").unwrap();
        let mut settings = sandboxed_settings(root.path(), tools.path());
        settings.tools.claude.skills_path = blocker.join("skills");
        let result =
            resync_suite_binding_for_tool(&settings, &suites, &bindings, ToolId::Claude).unwrap();

        assert!(!result.apply_result.errors.is_empty());
        let persisted = bindings.get(ToolId::Claude).unwrap().unwrap();
        assert_eq!(persisted.suite_id, selected.id);
        assert_eq!(persisted.manual_item_ids, vec!["skill:recorded"]);
    }
}
