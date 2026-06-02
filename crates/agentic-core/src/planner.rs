//! Read-only state inspection. Computes the current per-tool [`LinkState`] for
//! each scanned item against fresh disk state. No filesystem writes. The plan
//! and apply stages build on top of this. See `ARCHITECTURE.projection.md`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapter_registry::{ProjectionMode, ResolvedAdapter};
use crate::managed_copy;
use crate::model::{
    CapabilityItem, CapabilityKind, LinkState, OperationKind, PlannedOperation, ToolCapabilityState,
};
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
            let root = adapter.base_path_for(item.kind)?;
            let (state, current) = inspect_managed_copy(item, &target, root);
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

fn inspect_managed_copy(
    item: &CapabilityItem,
    target: &Path,
    target_root: &Path,
) -> (LinkState, Option<PathBuf>) {
    let meta = match fs::symlink_metadata(target) {
        Ok(m) => m,
        Err(_) => return (LinkState::Disabled, None),
    };
    if meta.file_type().is_symlink() {
        // A legacy symlink at a managed target: stale if it points at the right
        // source (refresh into a copy), foreign otherwise.
        let dest = fs::read_link(target).ok();
        let state = if dest.as_deref() == Some(item.source_path.as_path()) {
            LinkState::Stale
        } else {
            LinkState::ForeignLink
        };
        return (state, dest);
    }
    // Skills are directory copies; agents/rules are file copies. A type mismatch
    // at the target means a user-owned thing occupies it.
    let expect_dir = item.source_path.is_dir();
    if expect_dir != meta.is_dir() {
        return (LinkState::ForeignFile, None);
    }
    match managed_copy::read_entry(target_root, target) {
        Some(entry) if entry.source_path == item.source_path => {
            match (
                managed_copy::content_hash(target),
                managed_copy::content_hash(&item.source_path),
            ) {
                (Some(th), Some(sh)) if th == sh && entry.source_hash == sh => {
                    (LinkState::Enabled, Some(target.to_path_buf()))
                }
                (Some(_), Some(_)) => (LinkState::Stale, Some(target.to_path_buf())),
                // Expected content (e.g. SKILL.md) is missing on either side.
                _ => (LinkState::Broken, Some(target.to_path_buf())),
            }
        }
        // Manifest attributes the copy to a different source.
        Some(_) => (LinkState::ForeignFile, None),
        // Real file/dir without a manifest entry — user-owned.
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

/// Diff desired-vs-current and emit the per-item operations for one tool, then
/// resolve projection-target collisions. Only `link_sync` and `file_sync` items
/// are planned here; markdown rules and hooks flow through their own sync paths.
/// State is re-inspected internally from fresh disk state.
pub fn build_plan(
    items: &[CapabilityItem],
    adapter: &ResolvedAdapter,
    desired_enabled: &HashMap<String, bool>,
    force: bool,
) -> Vec<PlannedOperation> {
    let mut ops: Vec<PlannedOperation> = Vec::new();
    for item in items {
        let Some(mode) = adapter.projection_mode_for(item.kind) else {
            continue;
        };
        if !matches!(mode, ProjectionMode::LinkSync | ProjectionMode::FileSync) {
            continue;
        }
        let Some(state) = inspect_item(item, adapter) else {
            continue;
        };
        let Some(root) = adapter.base_path_for(item.kind) else {
            continue;
        };
        let desired = desired_enabled.get(&item.id).copied().unwrap_or(false);
        let managed = matches!(mode, ProjectionMode::FileSync);
        if let Some(op) = diff_op(item, &state, root, desired, managed, force) {
            ops.push(op);
        }
    }
    resolve_target_collisions(&mut ops);
    ops
}

fn diff_op(
    item: &CapabilityItem,
    state: &ToolCapabilityState,
    target_root: &Path,
    desired: bool,
    managed: bool,
    force: bool,
) -> Option<PlannedOperation> {
    use LinkState::{Broken, Disabled, Enabled, ForeignFile, ForeignLink, Stale};
    use OperationKind::{
        CreateLink, CreateManagedCopy, RemoveLink, RemoveManagedCopy, ReplaceLink,
        ReplaceManagedCopy, SkipConflict,
    };

    // A confirmed take-over: replace the blocking real file/dir from source.
    let mut takeover = false;
    let (kind, reason, with_source) = match (state.state, desired) {
        (Enabled, true) | (Disabled, false) | (ForeignFile, false) => return None,
        (Enabled, false) => (
            if managed {
                RemoveManagedCopy
            } else {
                RemoveLink
            },
            "Disable",
            false,
        ),
        (Disabled, true) => (
            if managed {
                CreateManagedCopy
            } else {
                CreateLink
            },
            "Enable",
            true,
        ),
        (Broken, true) => (
            if managed {
                ReplaceManagedCopy
            } else {
                ReplaceLink
            },
            "Repair broken projection",
            true,
        ),
        (Broken, false) => (
            if managed {
                RemoveManagedCopy
            } else {
                RemoveLink
            },
            "Remove broken projection",
            false,
        ),
        (Stale, true) => (ReplaceManagedCopy, "Refresh stale copy from source", true),
        (Stale, false) => (RemoveManagedCopy, "Remove stale copy", false),
        (ForeignLink, true) => (
            if managed {
                ReplaceManagedCopy
            } else {
                ReplaceLink
            },
            "Take over projection owned by another source",
            true,
        ),
        (ForeignLink, false) => (
            SkipConflict,
            "Projection owned by another source; not removing",
            false,
        ),
        (ForeignFile, true) => {
            if force {
                takeover = true;
                (
                    if managed {
                        ReplaceManagedCopy
                    } else {
                        ReplaceLink
                    },
                    "Take over target, replacing a real file",
                    true,
                )
            } else {
                (SkipConflict, "A real file blocks projection", false)
            }
        }
    };

    Some(PlannedOperation {
        tool: state.tool,
        item_id: item.id.clone(),
        target_root: target_root.to_path_buf(),
        target_path: state.target_path.clone(),
        source_path: with_source.then(|| item.source_path.clone()),
        kind,
        reason: reason.to_string(),
        force: takeover,
    })
}

fn writes_target(kind: OperationKind) -> bool {
    matches!(
        kind,
        OperationKind::CreateLink
            | OperationKind::ReplaceLink
            | OperationKind::CreateManagedCopy
            | OperationKind::ReplaceManagedCopy
    )
}

/// Group writing ops by target; for any clash, the lowest `item_id` wins and the
/// rest become `skip_conflict`. Covers cross-source clashes and Claude's flat
/// basename collisions deterministically.
fn resolve_target_collisions(ops: &mut [PlannedOperation]) {
    let mut groups: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, op) in ops.iter().enumerate() {
        if writes_target(op.kind) {
            groups.entry(op.target_path.clone()).or_default().push(i);
        }
    }
    for idxs in groups.into_values() {
        if idxs.len() < 2 {
            continue;
        }
        let mut sorted = idxs;
        sorted.sort_by(|&a, &b| ops[a].item_id.cmp(&ops[b].item_id));
        let winner = ops[sorted[0]].item_id.clone();
        for &i in sorted.iter().skip(1) {
            ops[i].kind = OperationKind::SkipConflict;
            ops[i].source_path = None;
            ops[i].reason = format!("Target path already taken by '{winner}'");
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
        let root = dir.path().join("cursor-agents");
        let target = root.join("agent.md");
        let it = item(CapabilityKind::Agent, "agent.md", source.clone());

        // Disabled.
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::Disabled
        );

        // ForeignFile: file without a manifest entry.
        write(&target, "agent v1");
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::ForeignFile
        );

        // Enabled: manifest entry matches source + content hash.
        managed_copy::write_managed_copy(&source, &target, &root, &it.id, false).unwrap();
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::Enabled
        );

        // Stale: content drifts from the recorded hash.
        write(&target, "agent v2 edited");
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::Stale
        );
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

    #[test]
    fn build_plan_enable_creates_link_and_skips_rules() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent");
        let agents_dir = dir.path().join("codex-agents");
        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "foo.md", source.clone());
        let rule = item(
            CapabilityKind::Rule,
            "general/precise.mdc",
            dir.path().join("src/precise.mdc"),
        );

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);
        desired.insert(rule.id.clone(), true); // markdown rule: not planned here

        let ops = build_plan(&[agent.clone(), rule], &adapter, &desired, false);
        assert_eq!(ops.len(), 1, "rule excluded from build_plan");
        assert_eq!(ops[0].kind, OperationKind::CreateLink);
        assert_eq!(ops[0].item_id, agent.id);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));
    }

    #[test]
    fn foreign_file_link_takeover_only_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent body");
        let agents_dir = dir.path().join("codex-agents");
        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "foo.md", source.clone());
        // A real (user-owned) file occupies the target -> ForeignFile.
        let target = adapter.target_path_for(&agent).unwrap();
        write(&target, "user owned");
        assert_eq!(inspect_symlink(&agent, &target).0, LinkState::ForeignFile);

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);

        // Without force: surfaced as a skip, nothing destructive.
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::SkipConflict);
        assert!(!ops[0].force);
        assert!(ops[0].source_path.is_none());

        // With force: a take-over ReplaceLink carrying the source + force flag.
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, true);
        assert_eq!(ops[0].kind, OperationKind::ReplaceLink);
        assert!(ops[0].force);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));
    }

    #[test]
    fn foreign_file_managed_takeover_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        write(&source, "agent body");
        let agents_dir = dir.path().join("cursor-agents");
        // Cursor agents project as managed copies (file_sync).
        let adapter = adapter_with(ToolId::Cursor, |s| {
            s.tools.cursor.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "agent.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        // A real file with no manifest entry -> ForeignFile.
        write(&target, "user owned");
        assert_eq!(
            inspect_managed_copy(&agent, &target, &agents_dir).0,
            LinkState::ForeignFile
        );

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);

        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, true);
        assert_eq!(ops[0].kind, OperationKind::ReplaceManagedCopy);
        assert!(ops[0].force);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));
    }

    #[test]
    fn build_plan_claude_flat_collision_first_id_wins() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("claude-skills");
        let adapter = adapter_with(ToolId::Claude, |s| {
            s.tools.claude.skills_path = skills_dir.clone();
        });
        // Two skills with different nesting but the same basename -> same flat target.
        let a = item(
            CapabilityKind::Skill,
            "alpha/shared",
            dir.path().join("src/a/shared"),
        );
        let b = item(
            CapabilityKind::Skill,
            "beta/shared",
            dir.path().join("src/b/shared"),
        );
        let mut desired = HashMap::new();
        desired.insert(a.id.clone(), true);
        desired.insert(b.id.clone(), true);

        let ops = build_plan(&[a, b], &adapter, &desired, false);
        // Claude skills project as managed copies, so the winner is a create-copy.
        let creates = ops
            .iter()
            .filter(|o| o.kind == OperationKind::CreateManagedCopy)
            .count();
        let skips = ops
            .iter()
            .filter(|o| o.kind == OperationKind::SkipConflict)
            .count();
        assert_eq!(creates, 1, "one wins the flat target");
        assert_eq!(skips, 1, "the other is a skip_conflict");
    }

    #[cfg(unix)]
    #[test]
    fn disable_enabled_link_emits_remove_link() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent");
        let agents_dir = dir.path().join("codex-agents");
        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "foo.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        symlink(&source, &target).unwrap();

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), false);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, OperationKind::RemoveLink);
        assert!(ops[0].source_path.is_none());
    }

    #[test]
    fn disable_enabled_managed_copy_emits_remove_managed_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        write(&source, "agent v1");
        let agents_dir = dir.path().join("cursor-agents");
        let adapter = adapter_with(ToolId::Cursor, |s| {
            s.tools.cursor.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "agent.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        managed_copy::write_managed_copy(&source, &target, &agents_dir, &agent.id, false).unwrap();

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), false);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, OperationKind::RemoveManagedCopy);
        assert!(ops[0].source_path.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn broken_link_repairs_with_replace_or_removes() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent");
        let agents_dir = dir.path().join("codex-agents");
        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "foo.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        symlink(&source, &target).unwrap();
        // Source vanishes: the symlink now dangles -> Broken.
        fs::remove_file(&source).unwrap();
        assert_eq!(inspect_symlink(&agent, &target).0, LinkState::Broken);

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::ReplaceLink);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));

        desired.insert(agent.id.clone(), false);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::RemoveLink);
        assert!(ops[0].source_path.is_none());
    }

    #[test]
    fn stale_managed_copy_refreshes_or_removes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        write(&source, "v1");
        let agents_dir = dir.path().join("cursor-agents");
        let adapter = adapter_with(ToolId::Cursor, |s| {
            s.tools.cursor.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "agent.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        managed_copy::write_managed_copy(&source, &target, &agents_dir, &agent.id, false).unwrap();
        // Edit the copy so it drifts from the recorded hash -> Stale.
        write(&target, "v2 edited");
        assert_eq!(
            inspect_managed_copy(&agent, &target, &agents_dir).0,
            LinkState::Stale
        );

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::ReplaceManagedCopy);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));

        desired.insert(agent.id.clone(), false);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::RemoveManagedCopy);
    }

    #[cfg(unix)]
    #[test]
    fn foreign_link_replaces_on_enable_skips_on_disable() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        write(&source, "agent");
        let other = dir.path().join("src/other.md");
        write(&other, "other");
        let agents_dir = dir.path().join("codex-agents");
        let adapter = adapter_with(ToolId::Codex, |s| {
            s.tools.codex.agents_path = agents_dir.clone();
        });
        let agent = item(CapabilityKind::Agent, "foo.md", source.clone());
        let target = adapter.target_path_for(&agent).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        symlink(&other, &target).unwrap();
        assert_eq!(inspect_symlink(&agent, &target).0, LinkState::ForeignLink);

        let mut desired = HashMap::new();
        desired.insert(agent.id.clone(), true);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::ReplaceLink);
        assert_eq!(ops[0].source_path.as_ref(), Some(&source));

        desired.insert(agent.id.clone(), false);
        let ops = build_plan(std::slice::from_ref(&agent), &adapter, &desired, false);
        assert_eq!(ops[0].kind, OperationKind::SkipConflict);
        assert!(ops[0].source_path.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn managed_copy_legacy_symlink_is_stale_or_foreign() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        write(&source, "v1");
        let root = dir.path().join("cursor-agents");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("agent.md");
        let it = item(CapabilityKind::Agent, "agent.md", source.clone());

        // A legacy symlink pointing at the right source: refresh into a copy.
        symlink(&source, &target).unwrap();
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::Stale
        );

        // Pointing elsewhere: owned by another source.
        fs::remove_file(&target).unwrap();
        let other = dir.path().join("src/other.md");
        write(&other, "other");
        symlink(&other, &target).unwrap();
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::ForeignLink
        );
    }

    #[test]
    fn managed_copy_missing_skill_md_is_broken() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/skill");
        fs::create_dir_all(&source).unwrap();
        write(&source.join("SKILL.md"), "# s");
        let root = dir.path().join("claude-skills");
        let target = root.join("skill");
        let it = item(CapabilityKind::Skill, "skill", source.clone());
        managed_copy::write_managed_copy(&source, &target, &root, &it.id, false).unwrap();
        // The copy's SKILL.md disappears: expected content is gone -> Broken.
        fs::remove_file(target.join("SKILL.md")).unwrap();
        assert_eq!(
            inspect_managed_copy(&it, &target, &root).0,
            LinkState::Broken
        );
    }
}
