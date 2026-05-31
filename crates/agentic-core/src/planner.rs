//! Read-only state inspection. Computes the current per-tool [`LinkState`] for
//! each scanned item against fresh disk state. No filesystem writes. The plan
//! and apply stages build on top of this. See `ARCHITECTURE.projection.md`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::adapter_registry::{ProjectionMode, ResolvedAdapter};
use crate::managed_copy;
use crate::model::{CapabilityItem, CapabilityKind, LinkState, ToolCapabilityState};
use crate::rule_sync::{self, BlockState};

/// Inspect every applicable item for one tool. Hook items are skipped — their
/// state is owned by the (forthcoming) `hook_sync` `json_section` path.
pub fn inspect_tool(
    items: &[CapabilityItem],
    adapter: &ResolvedAdapter,
) -> Vec<ToolCapabilityState> {
    items
        .iter()
        .filter_map(|item| inspect_item(item, adapter))
        .collect()
}

fn inspect_item(item: &CapabilityItem, adapter: &ResolvedAdapter) -> Option<ToolCapabilityState> {
    if item.kind == CapabilityKind::Hook {
        return None;
    }
    let mode = adapter.projection_mode_for(item.kind)?;

    let (target_path, state, current) = match mode {
        ProjectionMode::MarkdownSectionSync => {
            let instructions = adapter.instructions_path.clone()?;
            let state = inspect_markdown_rule(&instructions, &item.relative_path);
            let current = (state == LinkState::Enabled).then(|| instructions.clone());
            (instructions, state, current)
        }
        ProjectionMode::FileSync => {
            let target = adapter.target_path_for(item)?;
            let (state, current) = inspect_managed_copy(item, &target);
            (target, state, current)
        }
        ProjectionMode::LinkSync => {
            let target = adapter.target_path_for(item)?;
            let (state, current) = inspect_symlink(item, &target);
            (target, state, current)
        }
        // Hooks are filtered out above; no other kind resolves to JsonSection.
        ProjectionMode::JsonSection => return None,
    };

    Some(ToolCapabilityState {
        tool: adapter.tool_id,
        item_id: item.id.clone(),
        target_path,
        state,
        current_link_target: current,
    })
}

fn inspect_symlink(item: &CapabilityItem, target: &Path) -> (LinkState, Option<PathBuf>) {
    let meta = match fs::symlink_metadata(target) {
        Ok(m) => m,
        Err(_) => return (LinkState::Disabled, None),
    };
    if !meta.file_type().is_symlink() {
        // A real file or directory occupies the target.
        return (LinkState::ForeignFile, None);
    }
    let link = fs::read_link(target).ok();
    let resolved_exists = target.exists(); // follows the symlink
    match link {
        Some(dest) => {
            let state = if dest == item.source_path {
                if resolved_exists {
                    LinkState::Enabled
                } else {
                    LinkState::Broken
                }
            } else if resolved_exists {
                LinkState::ForeignLink
            } else {
                LinkState::Broken
            };
            (state, Some(dest))
        }
        None => (LinkState::Broken, None),
    }
}

fn inspect_managed_copy(item: &CapabilityItem, target: &Path) -> (LinkState, Option<PathBuf>) {
    let meta = match fs::symlink_metadata(target) {
        Ok(m) => m,
        Err(_) => return (LinkState::Disabled, None),
    };
    if meta.file_type().is_symlink() {
        return (LinkState::ForeignLink, fs::read_link(target).ok());
    }
    if !meta.is_file() {
        return (LinkState::ForeignFile, None);
    }
    match managed_copy::read_meta(target) {
        Some(m) if m.source_path == item.source_path => match managed_copy::hash_file(target) {
            Ok(hash) if hash == m.source_hash => (LinkState::Enabled, Some(target.to_path_buf())),
            _ => (LinkState::Stale, Some(target.to_path_buf())),
        },
        // Sidecar attributes the copy to a different source.
        Some(_) => (LinkState::ForeignLink, Some(target.to_path_buf())),
        // Real file without our sidecar — user-owned.
        None => (LinkState::ForeignFile, None),
    }
}

fn inspect_markdown_rule(instructions: &Path, relative_path: &Path) -> LinkState {
    let content = match fs::read_to_string(instructions) {
        Ok(c) => c,
        Err(_) => return LinkState::Disabled,
    };
    match rule_sync::read_block(&content) {
        BlockState::Missing => LinkState::Disabled,
        BlockState::Malformed => LinkState::Broken,
        BlockState::Present(inner) => {
            if rule_sync::block_lists_rule(&inner, relative_path) {
                LinkState::Enabled
            } else {
                LinkState::Disabled
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_registry;
    use crate::model::ToolId;
    use crate::settings::Settings;
    use std::fs;

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

    fn adapter_with(tool: ToolId, mutate: impl FnOnce(&mut Settings)) -> ResolvedAdapter {
        let mut s = Settings::default();
        mutate(&mut s);
        adapter_registry::resolve(&s, tool)
    }

    #[cfg(unix)]
    #[test]
    fn symlink_states() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent body");
        let agents_dir = dir.path().join("codex-agents");
        fs::create_dir_all(&agents_dir).unwrap();

        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let it = item(CapabilityKind::Agent, "foo.md", source.clone());
        let target = adapter.target_path_for(&it).unwrap();

        // Disabled: no target.
        assert_eq!(inspect_symlink(&it, &target).0, LinkState::Disabled);

        // Enabled: symlink to the correct source.
        symlink(&source, &target).unwrap();
        assert_eq!(inspect_symlink(&it, &target).0, LinkState::Enabled);

        // ForeignLink: symlink to a different existing path.
        fs::remove_file(&target).unwrap();
        let other = dir.path().join("src/other.md");
        write(&other, "other");
        symlink(&other, &target).unwrap();
        assert_eq!(inspect_symlink(&it, &target).0, LinkState::ForeignLink);

        // Broken: symlink to a missing path.
        fs::remove_file(&target).unwrap();
        symlink(dir.path().join("src/gone.md"), &target).unwrap();
        assert_eq!(inspect_symlink(&it, &target).0, LinkState::Broken);

        // ForeignFile: a real file at the target.
        fs::remove_file(&target).unwrap();
        write(&target, "real file");
        assert_eq!(inspect_symlink(&it, &target).0, LinkState::ForeignFile);
    }

    #[test]
    fn managed_copy_states() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        write(&source, "agent v1");
        let target = dir.path().join("cursor-agents/agent.md");
        let it = item(CapabilityKind::Agent, "agent.md", source.clone());

        // Disabled.
        assert_eq!(inspect_managed_copy(&it, &target).0, LinkState::Disabled);

        // ForeignFile: file without sidecar.
        write(&target, "agent v1");
        assert_eq!(inspect_managed_copy(&it, &target).0, LinkState::ForeignFile);

        // Enabled: sidecar matches source + content hash.
        let meta = managed_copy::ManagedCopyMeta {
            source_path: source.clone(),
            source_hash: managed_copy::hash_file(&target).unwrap(),
            synced_at: "2026-05-31T00:00:00Z".into(),
        };
        write(
            &managed_copy::meta_path(&target),
            &serde_json::to_string(&meta).unwrap(),
        );
        assert_eq!(inspect_managed_copy(&it, &target).0, LinkState::Enabled);

        // Stale: content drifts from the recorded hash.
        write(&target, "agent v2 edited");
        assert_eq!(inspect_managed_copy(&it, &target).0, LinkState::Stale);
    }

    #[test]
    fn markdown_rule_states() {
        let dir = tempfile::tempdir().unwrap();
        let instr = dir.path().join("CLAUDE.md");
        let adapter = adapter_with(ToolId::Claude, |s| {
            s.tools.claude.instructions_path = Some(instr.clone());
        });
        let rule = item(
            CapabilityKind::Rule,
            "general/precise.mdc",
            dir.path().join("src/general/precise.mdc"),
        );

        // Disabled: file missing.
        let states = inspect_tool(std::slice::from_ref(&rule), &adapter);
        assert_eq!(states[0].state, LinkState::Disabled);

        // Enabled: rule listed in the managed block.
        let content = format!(
            "# CLAUDE\n\n{}\n### general/precise.mdc\n\nbody\n{}\n",
            rule_sync::BLOCK_START,
            rule_sync::BLOCK_END
        );
        write(&instr, &content);
        let states = inspect_tool(std::slice::from_ref(&rule), &adapter);
        assert_eq!(states[0].state, LinkState::Enabled);
        assert_eq!(states[0].target_path, instr);

        // Broken: malformed markers.
        write(&instr, rule_sync::BLOCK_START);
        let states = inspect_tool(std::slice::from_ref(&rule), &adapter);
        assert_eq!(states[0].state, LinkState::Broken);
    }

    #[test]
    fn hooks_are_skipped_by_inspect() {
        let s = Settings::default();
        let adapter = adapter_registry::resolve(&s, ToolId::Codex);
        let hook = item(
            CapabilityKind::Hook,
            "auto-format-after-edit",
            PathBuf::from("/src/hooks/auto-format-after-edit"),
        );
        assert!(inspect_tool(std::slice::from_ref(&hook), &adapter).is_empty());
    }
}
