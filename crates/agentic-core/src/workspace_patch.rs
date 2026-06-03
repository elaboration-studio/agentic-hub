//! Workspace-scope projection: hard-copy a suite into a per-project directory
//! with a manifest that cleans the prior cycle on next apply. Skills/agents and
//! Cursor rules are copied (symlinks dereferenced); Codex/Claude rules use the
//! managed markdown block; hooks use the managed JSON section.
//!
//! All writes are confined to the workspace via a canonical-path guard. See
//! `docs/tech/modules/workspace-patch.md`.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapter_registry::{create_workspace_adapter, ProjectionMode, ResolvedAdapter};
use crate::error::{CoreError, Result};
use crate::hook_sync::{self, HookManifest};
use crate::managed_copy::now_iso8601;
use crate::model::{CapabilityItem, CapabilityKind, SuiteDefinition, ToolId, WorkspacePatchResult};
#[cfg(test)]
use crate::model::{SourceRef, SuiteCapabilityRef};
use crate::rule_sync;

const SECTION_SENTINEL: &str = "::managed-section";
const HOOKS_SENTINEL: &str = "::managed-hooks";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SuiteRef {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(default = "one")]
    version: u32,
    applied_at: String,
    tool: ToolId,
    suite: SuiteRef,
    #[serde(default)]
    paths: Vec<String>,
}

fn one() -> u32 {
    1
}

fn manifest_dir(ws: &Path) -> PathBuf {
    ws.join(".agentic-hub")
}

fn manifest_path(ws: &Path) -> PathBuf {
    manifest_dir(ws).join("workspace-patch.json")
}

/// Tolerant read: missing or malformed → `None` (prior state treated as empty).
fn read_manifest(ws: &Path) -> Option<Manifest> {
    let content = fs::read_to_string(manifest_path(ws)).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_manifest(ws: &Path, manifest: &Manifest) -> Result<()> {
    let dir = manifest_dir(ws);
    fs::create_dir_all(&dir)?;
    let path = manifest_path(ws);
    let json = serde_json::to_string_pretty(manifest)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Confirm `target` resolves strictly inside `real_ws` (never equal to it).
/// Resolves the nearest existing ancestor so symlinked ancestors cannot escape.
fn guard_in_workspace(target: &Path, real_ws: &Path) -> std::result::Result<(), String> {
    let mut existing = target;
    let mut tail = PathBuf::new();
    let real_existing = loop {
        if let Ok(real) = existing.canonicalize() {
            break real;
        }
        let file = existing
            .file_name()
            .ok_or_else(|| format!("invalid path: {}", target.display()))?;
        tail = Path::new(file).join(&tail);
        existing = existing
            .parent()
            .ok_or_else(|| format!("invalid path: {}", target.display()))?;
    };
    let real_target = real_existing.join(&tail);
    if real_target == *real_ws || !real_target.starts_with(real_ws) {
        return Err(format!("target outside workspace: {}", target.display()));
    }
    Ok(())
}

/// Remove empty parent directories up to (but never including) `up_to`.
fn prune_empty_parents(start: &Path, up_to: &Path) {
    let mut current = start.parent();
    while let Some(p) = current {
        if p == up_to {
            break;
        }
        let is_empty = fs::read_dir(p)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !is_empty || fs::remove_dir(p).is_err() {
            break;
        }
        current = p.parent();
    }
}

/// Recursive copy that dereferences symlinks — no symlink crosses into the
/// workspace.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                copy_tree(&from, &to)?;
            } else {
                fs::copy(&from, &to)?;
            }
        }
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

fn remove_path(target: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(target) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(target),
        Ok(_) => fs::remove_file(target),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn rel_unix(target: &Path, real_ws: &Path) -> String {
    target
        .strip_prefix(real_ws)
        .unwrap_or(target)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Apply a suite into a workspace directory (full hard-overwrite cycle).
pub fn apply_workspace_patch(
    workspace_dir: &Path,
    tool: ToolId,
    suite: &SuiteDefinition,
    items: &[CapabilityItem],
    manifests: &HashMap<String, HookManifest>,
) -> Result<WorkspacePatchResult> {
    let meta = fs::metadata(workspace_dir)
        .map_err(|_| CoreError::PathNotFound(workspace_dir.to_path_buf()))?;
    if !meta.is_dir() {
        return Err(CoreError::NotADirectory(workspace_dir.to_path_buf()));
    }
    let real_ws = workspace_dir.canonicalize()?;
    let adapter = create_workspace_adapter(tool, &real_ws);

    let mut result = WorkspacePatchResult {
        tool,
        workspace_dir: real_ws.clone(),
        suite_id: suite.id.clone(),
        suite_name: suite.name.clone(),
        applied: Vec::new(),
        removed: Vec::new(),
        skipped_stale_ids: Vec::new(),
        notes: Vec::new(),
        errors: Vec::new(),
    };

    // Resolve suite capability refs against the scan (present sources only).
    // A qualified ref whose source is not active here is preserved, never
    // mis-resolved onto a same-named capability from a different source.
    let by_id: HashMap<&str, &CapabilityItem> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut queued: Vec<&CapabilityItem> = Vec::new();
    for cap in &suite.capabilities {
        match by_id.get(cap.cap.as_str()) {
            Some(item)
                if cap
                    .source
                    .as_ref()
                    .map_or(true, |s| s.matches(&item.source)) =>
            {
                queued.push(item)
            }
            Some(_) => result.notes.push(format!(
                "{} skipped: its source is not active on this machine.",
                cap.cap
            )),
            None => result.skipped_stale_ids.push(cap.cap.clone()),
        }
    }

    // Clean the prior cycle first.
    if let Some(prior) = read_manifest(&real_ws) {
        cleanup_prior(&prior, &real_ws, &mut result);
    } else if manifest_path(&real_ws).exists() {
        result
            .notes
            .push("Prior manifest was malformed; treated as empty.".to_string());
    }

    let mut paths: Vec<String> = Vec::new();
    let mut md_rules: Vec<&CapabilityItem> = Vec::new();
    let mut hook_pairs: Vec<(&CapabilityItem, &HookManifest)> = Vec::new();

    for item in &queued {
        match item.kind {
            CapabilityKind::Skill => {
                let target = adapter.skills_path.join(&item.relative_path);
                copy_into(
                    &item.source_path,
                    &target,
                    &real_ws,
                    &mut paths,
                    &mut result,
                );
            }
            CapabilityKind::Agent => {
                let target = adapter.agents_path.join(&item.relative_path);
                copy_into(
                    &item.source_path,
                    &target,
                    &real_ws,
                    &mut paths,
                    &mut result,
                );
            }
            CapabilityKind::Rule => {
                if tool == ToolId::Cursor {
                    let target = adapter.rules_path.join(&item.relative_path);
                    copy_into(
                        &item.source_path,
                        &target,
                        &real_ws,
                        &mut paths,
                        &mut result,
                    );
                } else {
                    md_rules.push(item);
                }
            }
            CapabilityKind::Hook => {
                if adapter.hooks_enabled {
                    if let Some(m) = manifests.get(&item.id) {
                        if m.effective_targets().contains(&tool) {
                            hook_pairs.push((item, m));
                        }
                    }
                }
            }
        }
    }

    sync_markdown(&adapter, &real_ws, &md_rules, &mut paths, &mut result);
    sync_hooks(&adapter, &real_ws, &hook_pairs, &mut paths, &mut result);

    let manifest = Manifest {
        version: 1,
        applied_at: now_iso8601(),
        tool,
        suite: SuiteRef {
            id: suite.id.clone(),
            name: suite.name.clone(),
        },
        paths: paths.clone(),
    };
    write_manifest(&real_ws, &manifest)?;

    result.applied = paths;
    Ok(result)
}

/// Copy a source skill/agent/rule into `target`, recording its workspace-relative
/// path. Guard failures and IO errors are recorded and skipped, not fatal.
fn copy_into(
    source: &Path,
    target: &Path,
    real_ws: &Path,
    paths: &mut Vec<String>,
    result: &mut WorkspacePatchResult,
) {
    if let Err(e) = guard_in_workspace(target, real_ws) {
        result.errors.push(e);
        return;
    }
    match copy_tree(source, target) {
        Ok(()) => paths.push(rel_unix(target, real_ws)),
        Err(e) => result
            .errors
            .push(format!("copy {}: {e}", target.display())),
    }
}

fn sync_markdown(
    adapter: &ResolvedAdapter,
    real_ws: &Path,
    rules: &[&CapabilityItem],
    paths: &mut Vec<String>,
    result: &mut WorkspacePatchResult,
) {
    if adapter.projection_mode_for(CapabilityKind::Rule)
        != Some(ProjectionMode::MarkdownSectionSync)
    {
        return;
    }
    let Some(instructions) = adapter.instructions_path.clone() else {
        return;
    };
    if let Err(e) = guard_in_workspace(&instructions, real_ws) {
        result.errors.push(e);
        return;
    }
    match rule_sync::sync_markdown_rules(&instructions, rules) {
        Ok(_) => paths.push(format!(
            "{}{SECTION_SENTINEL}",
            rel_unix(&instructions, real_ws)
        )),
        Err(e) => result.errors.push(e.message),
    }
}

fn sync_hooks(
    adapter: &ResolvedAdapter,
    real_ws: &Path,
    pairs: &[(&CapabilityItem, &HookManifest)],
    paths: &mut Vec<String>,
    result: &mut WorkspacePatchResult,
) {
    let Some(hooks_file) = adapter.hooks_file.clone() else {
        return;
    };
    if !adapter.hooks_enabled {
        return;
    }
    if let Err(e) = guard_in_workspace(&hooks_file, real_ws) {
        result.errors.push(e);
        return;
    }
    match hook_sync::sync_json_hooks(adapter, pairs) {
        Ok((_, notes)) => {
            result.notes.extend(notes);
            paths.push(format!(
                "{}{HOOKS_SENTINEL}",
                rel_unix(&hooks_file, real_ws)
            ));
        }
        Err(e) => result.errors.push(e.message),
    }
}

/// Undo the prior manifest: clear managed markdown/hook sections for the prior
/// tool and delete copied paths, pruning emptied parents.
fn cleanup_prior(prior: &Manifest, real_ws: &Path, result: &mut WorkspacePatchResult) {
    let prior_adapter = create_workspace_adapter(prior.tool, real_ws);
    for entry in &prior.paths {
        if let Some(file) = entry.strip_suffix(SECTION_SENTINEL) {
            let instructions = real_ws.join(file);
            let _ = rule_sync::sync_markdown_rules(&instructions, &[]);
            result.removed.push(entry.clone());
        } else if let Some(_file) = entry.strip_suffix(HOOKS_SENTINEL) {
            let _ = hook_sync::sync_json_hooks(&prior_adapter, &[]);
            result.removed.push(entry.clone());
        } else {
            let target = real_ws.join(entry);
            if guard_in_workspace(&target, real_ws).is_err() {
                continue;
            }
            if remove_path(&target).is_ok() {
                prune_empty_parents(&target, real_ws);
                result.removed.push(entry.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn item(kind: CapabilityKind, id: &str, rel: &str, source: PathBuf) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind,
            name: rel.to_string(),
            source_path: source,
            relative_path: PathBuf::from(rel),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            source: SourceRef {
                rel_home: "~/.agentic".into(),
                folder: ".agentic".into(),
            },
            valid: true,
            validation_errors: vec![],
        }
    }

    fn suite(caps: &[&str]) -> SuiteDefinition {
        SuiteDefinition {
            id: "s1".into(),
            name: "coding".into(),
            description: None,
            capabilities: caps.iter().map(|s| SuiteCapabilityRef::bare(*s)).collect(),
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn guard_rejects_out_of_workspace() {
        let ws = tempfile::tempdir().unwrap();
        let real = ws.path().canonicalize().unwrap();
        assert!(guard_in_workspace(&real.join("ok/file"), &real).is_ok());
        assert!(guard_in_workspace(&real, &real).is_err());
        assert!(guard_in_workspace(Path::new("/etc/passwd"), &real).is_err());
    }

    #[test]
    fn apply_copies_skill_and_rule_and_dereferences_symlink() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();

        // A skill folder containing a symlinked file.
        let skill_dir = src.path().join("skills/dev/tdd");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# tdd").unwrap();
        let real_target = src.path().join("real.txt");
        fs::write(&real_target, "linked").unwrap();
        symlink(&real_target, skill_dir.join("link.txt")).unwrap();

        let rule = src.path().join("precise.mdc");
        fs::write(&rule, "> rule").unwrap();

        let items = vec![
            item(CapabilityKind::Skill, "skill:dev/tdd", "dev/tdd", skill_dir),
            item(
                CapabilityKind::Rule,
                "rule:precise.mdc",
                "precise.mdc",
                rule,
            ),
        ];
        let manifests = HashMap::new();

        let result = apply_workspace_patch(
            ws.path(),
            ToolId::Cursor,
            &suite(&["skill:dev/tdd", "rule:precise.mdc", "skill:gone"]),
            &items,
            &manifests,
        )
        .unwrap();

        let real_ws = ws.path().canonicalize().unwrap();
        assert!(real_ws.join(".cursor/skills/dev/tdd/SKILL.md").is_file());
        let link = real_ws.join(".cursor/skills/dev/tdd/link.txt");
        assert!(link.is_file());
        assert!(
            !link.symlink_metadata().unwrap().file_type().is_symlink(),
            "symlink dereferenced to a real file"
        );
        assert!(real_ws.join(".cursor/rules/precise.mdc").is_file());
        assert_eq!(result.skipped_stale_ids, vec!["skill:gone"]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn apply_is_source_aware_for_qualified_refs() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let skill_dir = src.path().join("skills/dev/tdd");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# tdd").unwrap();
        let items = vec![item(
            CapabilityKind::Skill,
            "skill:dev/tdd",
            "dev/tdd",
            skill_dir,
        )];
        let skill_path = ".cursor/skills/dev/tdd/SKILL.md";

        // Qualified to a source that is NOT active here: preserved, not applied,
        // and never mis-resolved onto the local same-named item.
        let inactive = SuiteDefinition {
            id: "s".into(),
            name: "s".into(),
            description: None,
            capabilities: vec![SuiteCapabilityRef {
                cap: "skill:dev/tdd".into(),
                source: Some(SourceRef {
                    rel_home: "~/other".into(),
                    folder: "other".into(),
                }),
            }],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let result = apply_workspace_patch(
            ws.path(),
            ToolId::Cursor,
            &inactive,
            &items,
            &HashMap::new(),
        )
        .unwrap();
        let real_ws = ws.path().canonicalize().unwrap();
        assert!(
            !real_ws.join(skill_path).exists(),
            "inactive-source ref not applied"
        );
        assert!(
            result.skipped_stale_ids.is_empty(),
            "skipped as absent, not stale"
        );
        assert!(result.notes.iter().any(|n| n.contains("skill:dev/tdd")));

        // Qualified to the matching source: applied normally.
        let active = SuiteDefinition {
            id: "s".into(),
            name: "s".into(),
            description: None,
            capabilities: vec![SuiteCapabilityRef {
                cap: "skill:dev/tdd".into(),
                source: Some(SourceRef {
                    rel_home: "~/.agentic".into(),
                    folder: ".agentic".into(),
                }),
            }],
            is_base: false,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        apply_workspace_patch(ws.path(), ToolId::Cursor, &active, &items, &HashMap::new()).unwrap();
        assert!(
            real_ws.join(skill_path).is_file(),
            "matching-source ref applied"
        );
    }

    #[test]
    fn reapply_cleans_prior_payload() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let a = src.path().join("a");
        let b = src.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("SKILL.md"), "# a").unwrap();
        fs::write(b.join("SKILL.md"), "# b").unwrap();

        let items = vec![
            item(CapabilityKind::Skill, "skill:a", "a", a),
            item(CapabilityKind::Skill, "skill:b", "b", b),
        ];
        let m = HashMap::new();
        let real_ws = ws.path().canonicalize().unwrap();

        apply_workspace_patch(ws.path(), ToolId::Cursor, &suite(&["skill:a"]), &items, &m).unwrap();
        assert!(real_ws.join(".cursor/skills/a/SKILL.md").is_file());

        let result =
            apply_workspace_patch(ws.path(), ToolId::Cursor, &suite(&["skill:b"]), &items, &m)
                .unwrap();
        assert!(
            !real_ws.join(".cursor/skills/a").exists(),
            "prior skill removed"
        );
        assert!(real_ws.join(".cursor/skills/b/SKILL.md").is_file());
        assert!(result.removed.iter().any(|p| p.contains("skills/a")));
    }

    #[test]
    fn codex_rules_use_managed_markdown_section() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let rule = src.path().join("precise.mdc");
        fs::write(&rule, "> be precise").unwrap();
        let items = vec![item(
            CapabilityKind::Rule,
            "rule:precise.mdc",
            "precise.mdc",
            rule,
        )];
        let m = HashMap::new();

        apply_workspace_patch(
            ws.path(),
            ToolId::Codex,
            &suite(&["rule:precise.mdc"]),
            &items,
            &m,
        )
        .unwrap();
        let real_ws = ws.path().canonicalize().unwrap();
        let agents = fs::read_to_string(real_ws.join("AGENTS.md")).unwrap();
        assert!(agents.contains("agentic-hub:start"));
        assert!(agents.contains("be precise"));
    }

    fn hook_manifest(id: &str) -> HookManifest {
        HookManifest {
            id: id.to_string(),
            name: None,
            description: None,
            events: vec![hook_sync::HookEventSpec {
                name: hook_sync::HookCanonicalEvent::Stop,
                matcher: None,
            }],
            command: "${HOOK_DIR}/run.sh".to_string(),
            timeout: Some(30),
            loop_limit: None,
            targets: None,
        }
    }

    #[test]
    fn hook_in_suite_writes_cursor_hooks_file() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let hook_dir = src.path().join("hooks/fmt");
        fs::create_dir_all(&hook_dir).unwrap();

        let items = vec![item(CapabilityKind::Hook, "hook:fmt", "fmt", hook_dir)];
        let mut manifests = HashMap::new();
        manifests.insert("hook:fmt".to_string(), hook_manifest("fmt"));

        let result = apply_workspace_patch(
            ws.path(),
            ToolId::Cursor,
            &suite(&["hook:fmt"]),
            &items,
            &manifests,
        )
        .unwrap();

        let real_ws = ws.path().canonicalize().unwrap();
        assert!(real_ws.join(".cursor/hooks.json").is_file());
        assert!(
            result.applied.iter().any(|p| p.ends_with(HOOKS_SENTINEL)),
            "hooks sentinel recorded: {:?}",
            result.applied
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn tool_switch_clears_prior_markdown_section() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let rule = src.path().join("precise.mdc");
        fs::write(&rule, "> be precise").unwrap();
        let skill = src.path().join("s");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# s").unwrap();
        let items = vec![
            item(
                CapabilityKind::Rule,
                "rule:precise.mdc",
                "precise.mdc",
                rule,
            ),
            item(CapabilityKind::Skill, "skill:s", "s", skill),
        ];
        let m = HashMap::new();
        let real_ws = ws.path().canonicalize().unwrap();

        // Codex writes the managed markdown section into AGENTS.md.
        apply_workspace_patch(
            ws.path(),
            ToolId::Codex,
            &suite(&["rule:precise.mdc"]),
            &items,
            &m,
        )
        .unwrap();
        assert!(real_ws.join("AGENTS.md").exists());

        // Switching to Cursor cleans the prior Codex markdown section.
        apply_workspace_patch(ws.path(), ToolId::Cursor, &suite(&["skill:s"]), &items, &m).unwrap();
        let agents = real_ws.join("AGENTS.md");
        if agents.exists() {
            let body = fs::read_to_string(&agents).unwrap();
            assert!(
                !body.contains("agentic-hub:start"),
                "managed section cleared"
            );
        }
        assert!(real_ws.join(".cursor/skills/s/SKILL.md").is_file());
    }

    #[test]
    fn malformed_prior_manifest_is_noted() {
        let ws = tempfile::tempdir().unwrap();
        let real_ws = ws.path().canonicalize().unwrap();
        fs::create_dir_all(real_ws.join(".agentic-hub")).unwrap();
        fs::write(
            real_ws.join(".agentic-hub/workspace-patch.json"),
            "{ not valid json",
        )
        .unwrap();

        let result =
            apply_workspace_patch(ws.path(), ToolId::Cursor, &suite(&[]), &[], &HashMap::new())
                .unwrap();
        assert!(
            result
                .notes
                .iter()
                .any(|n| n.contains("Prior manifest was malformed")),
            "{:?}",
            result.notes
        );
    }

    #[test]
    fn missing_dir_and_file_path_error() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("nope");
        let err =
            apply_workspace_patch(&missing, ToolId::Cursor, &suite(&[]), &[], &HashMap::new())
                .unwrap_err();
        assert!(matches!(err, CoreError::PathNotFound(_)));

        let file = parent.path().join("a-file");
        fs::write(&file, "x").unwrap();
        let err = apply_workspace_patch(&file, ToolId::Cursor, &suite(&[]), &[], &HashMap::new())
            .unwrap_err();
        assert!(matches!(err, CoreError::NotADirectory(_)));
    }

    #[test]
    fn reapply_prunes_emptied_parent_dir() {
        let src = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let skill = src.path().join("nested");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# nested").unwrap();
        let items = vec![item(
            CapabilityKind::Skill,
            "skill:deep",
            "deep/nested",
            skill,
        )];
        let m = HashMap::new();
        let real_ws = ws.path().canonicalize().unwrap();

        apply_workspace_patch(
            ws.path(),
            ToolId::Cursor,
            &suite(&["skill:deep"]),
            &items,
            &m,
        )
        .unwrap();
        assert!(real_ws
            .join(".cursor/skills/deep/nested/SKILL.md")
            .is_file());

        // Reapply with an empty suite: the skill and its now-empty parent vanish.
        apply_workspace_patch(ws.path(), ToolId::Cursor, &suite(&[]), &items, &m).unwrap();
        assert!(!real_ws.join(".cursor/skills/deep/nested").exists());
        assert!(
            !real_ws.join(".cursor/skills/deep").exists(),
            "emptied parent pruned"
        );
    }
}
