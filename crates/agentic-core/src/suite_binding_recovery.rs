//! Safe re-application of one live suite-to-tool binding.

use std::collections::HashSet;

use crate::api::{
    apply_suite, enabled_item_ids, manual_extras_from_enabled, merge_base_caps, suite_matched_ids,
};
use crate::model::{ApplySuiteResult, CapabilityItem, SuiteBinding, SuiteDefinition};
use crate::settings::Settings;

/// Re-apply one live suite binding without changing its selected suite.
///
/// The effective set is the selected suite merged with the current base. Manual
/// extras are the union of the binding's persisted extras and extras currently
/// enabled on disk, so a stale persisted extra and a newly-added live extra are
/// both preserved through the full-reset apply.
pub fn resync_suite_binding(
    items: &[CapabilityItem],
    settings: &Settings,
    binding: &SuiteBinding,
    selected: &SuiteDefinition,
    base: Option<&SuiteDefinition>,
) -> ApplySuiteResult {
    let effective = merge_base_caps(selected, base);
    let matched = suite_matched_ids(items, &effective);
    let enabled = enabled_item_ids(items, settings, binding.tool_id);
    let mut manual: HashSet<String> = manual_extras_from_enabled(&enabled, &effective, items)
        .into_iter()
        .collect();
    manual.extend(
        binding
            .manual_item_ids
            .iter()
            .filter(|id| !matched.contains(*id) && items.iter().any(|item| item.id == **id))
            .cloned(),
    );
    let mut manual: Vec<String> = manual.into_iter().collect();
    manual.sort();
    let manual_refs: Vec<&str> = manual.iter().map(String::as_str).collect();
    apply_suite(items, settings, binding.tool_id, &effective, &manual_refs)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::api::{apply_suite, merge_base_caps, scan};
    use crate::model::ToolId;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn suite(name: &str, caps: &[&str]) -> SuiteDefinition {
        SuiteDefinition {
            id: name.into(),
            name: name.into(),
            description: None,
            capabilities: caps.iter().map(|cap| (*cap).into()).collect(),
            is_base: false,
            agent: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn resync_merges_base_and_preserves_live_and_recorded_manual_extras() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        write(
            &root.path().join("skills/selected/SKILL.md"),
            "# selected v1",
        );
        write(&root.path().join("skills/base/SKILL.md"), "# base v1");
        write(&root.path().join("skills/live/SKILL.md"), "# live");
        write(&root.path().join("skills/recorded/SKILL.md"), "# recorded");
        let settings = Settings::sandboxed(root.path(), tools.path());
        let scanned = scan(&settings);

        let selected = suite("selected", &["skill:selected"]);
        let mut base = suite("base", &["skill:base"]);
        base.is_base = true;
        let effective = merge_base_caps(&selected, Some(&base));
        apply_suite(
            &scanned.items,
            &settings,
            ToolId::Claude,
            &effective,
            &["skill:live", "skill:recorded"],
        );

        write(
            &root.path().join("skills/selected/SKILL.md"),
            "# selected v2",
        );
        write(&root.path().join("skills/base/SKILL.md"), "# base v2");
        let rescanned = scan(&settings);
        let binding = SuiteBinding {
            tool_id: ToolId::Claude,
            suite_id: selected.id.clone(),
            manual_item_ids: vec!["skill:recorded".into()],
        };

        let result = resync_suite_binding(
            &rescanned.items,
            &settings,
            &binding,
            &selected,
            Some(&base),
        );

        assert!(result.apply_result.errors.is_empty());
        assert_eq!(
            result.manual_item_ids,
            vec!["skill:live", "skill:recorded"],
            "live and previously recorded manual extras survive the re-sync"
        );
        assert_eq!(
            fs::read_to_string(settings.tools.claude.skills_path.join("selected/SKILL.md"))
                .unwrap(),
            "# selected v2"
        );
        assert_eq!(
            fs::read_to_string(settings.tools.claude.skills_path.join("base/SKILL.md")).unwrap(),
            "# base v2"
        );
        assert_eq!(
            binding.suite_id, selected.id,
            "binding identity is untouched"
        );
    }
}
