//! Read-only workspace inventory: walk a project's own per-tool directories
//! (`.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
//! `AGENTS.md`, `CLAUDE.md`) and report which agentic resources each tool
//! already has. The inverse of projection — this never writes. The result feeds
//! the manager matrix in read-only mode (only present resources are emitted, so
//! every cell shown is "enabled"). See `docs/tech/modules/workspace-inventory.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::adapter_registry::create_workspace_adapter;
use crate::model::{
    CapabilityItem, CapabilityKind, LinkState, ScanError, SourceRef, ToolCapabilityState, ToolId,
};
use crate::paths::tildify;

/// Bounded walk depth; mirrors the shared-root scanner guard against cycles.
const MAX_DEPTH: usize = 16;

/// Reserved folder name holding old versions of files. Never scanned.
const ARCHIVED: &str = "__archived__";

/// Synthetic source id/label for every workspace-discovered item. Workspace
/// scope has a single implicit source (the project itself).
const WORKSPACE_SOURCE_ID: &str = "workspace";
const WORKSPACE_SOURCE_LABEL: &str = "Workspace";

/// A read-only snapshot of one workspace's installed agentic resources, keyed
/// the same way the manager matrix consumes a global scan + inspect.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInventory {
    /// One row per distinct resource, deduped across tools (sorted by id).
    pub items: Vec<CapabilityItem>,
    /// One entry per `(tool, item)` actually present on disk, always `Enabled`.
    pub states: Vec<ToolCapabilityState>,
    pub errors: Vec<ScanError>,
}

/// A resource discovered in one tool's workspace directory.
struct Found {
    kind: CapabilityKind,
    id: String,
    name: String,
    relative_path: std::path::PathBuf,
    path: std::path::PathBuf,
}

/// Scan a workspace directory for the resources each of `tools` has installed.
/// Read-only: no filesystem writes. Missing tool dirs are not errors.
pub fn scan_workspace(ws: &Path, tools: &[ToolId]) -> WorkspaceInventory {
    let mut by_id: BTreeMap<String, CapabilityItem> = BTreeMap::new();
    let mut states: Vec<ToolCapabilityState> = Vec::new();
    let mut errors: Vec<ScanError> = Vec::new();
    let source = workspace_source_ref(ws);

    for &tool in tools {
        let adapter = create_workspace_adapter(tool, ws);
        if !adapter.enabled {
            continue;
        }

        let mut found: Vec<Found> = Vec::new();
        collect_skills(&adapter.skills_path, &mut found, &mut errors);
        collect_files(
            &adapter.agents_path,
            CapabilityKind::Agent,
            &mut found,
            &mut errors,
        );
        // Codex/Claude rules live in their managed instruction block, not a
        // rules dir; only Cursor keeps per-file rules under `.cursor/rules`.
        if tool == ToolId::Cursor {
            collect_files(
                &adapter.rules_path,
                CapabilityKind::Rule,
                &mut found,
                &mut errors,
            );
        }
        if let Some(instructions) = adapter.instructions_path.as_ref() {
            if instructions.is_file() {
                if let Some(file_name) = instructions.file_name().and_then(|s| s.to_str()) {
                    let rel = std::path::PathBuf::from(file_name);
                    found.push(Found {
                        kind: CapabilityKind::Rule,
                        id: format!("{}:{file_name}", CapabilityKind::Rule.id_prefix()),
                        name: file_name.to_string(),
                        relative_path: rel,
                        path: instructions.clone(),
                    });
                }
            }
        }

        for f in found {
            states.push(ToolCapabilityState {
                tool,
                item_id: f.id.clone(),
                target_path: f.path.clone(),
                state: LinkState::Enabled,
                current_link_target: None,
            });
            // First tool that owns the id provides the displayed source path.
            by_id.entry(f.id.clone()).or_insert_with(|| CapabilityItem {
                id: f.id,
                kind: f.kind,
                name: f.name,
                source_path: f.path,
                relative_path: f.relative_path,
                source_id: WORKSPACE_SOURCE_ID.to_string(),
                source_label: WORKSPACE_SOURCE_LABEL.to_string(),
                source: source.clone(),
                valid: true,
                validation_errors: Vec::new(),
            });
        }
    }

    WorkspaceInventory {
        items: by_id.into_values().collect(),
        states,
        errors,
    }
}

/// Portable identity of the workspace-as-source (display only).
fn workspace_source_ref(ws: &Path) -> SourceRef {
    let folder = ws
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    SourceRef {
        rel_home: tildify(ws),
        folder,
    }
}

/// Walk a skills directory; every folder containing `SKILL.md` is a skill.
fn collect_skills(base: &Path, out: &mut Vec<Found>, errors: &mut Vec<ScanError>) {
    if !base.is_dir() {
        return;
    }
    walk_skills(base, base, 0, out, errors);
}

fn walk_skills(
    base: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(ScanError {
                path: dir.to_path_buf(),
                message: e.to_string(),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if entry.file_name() == std::ffi::OsStr::new(ARCHIVED) {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            let name = rel
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(Found {
                kind: CapabilityKind::Skill,
                id: format!("{}:{}", CapabilityKind::Skill.id_prefix(), rel_unix(&rel)),
                name,
                relative_path: rel,
                path: path.clone(),
            });
        }
        walk_skills(base, &path, depth + 1, out, errors);
    }
}

/// Walk a file-based directory (agents or Cursor rules) for the kind's allowed
/// extensions; each matching file is one resource.
fn collect_files(
    base: &Path,
    kind: CapabilityKind,
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    if !base.is_dir() {
        return;
    }
    walk_files(base, base, kind, 0, out, errors);
}

fn walk_files(
    base: &Path,
    dir: &Path,
    kind: CapabilityKind,
    depth: usize,
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(ScanError {
                path: dir.to_path_buf(),
                message: e.to_string(),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != std::ffi::OsStr::new(ARCHIVED) {
                walk_files(base, &path, kind, depth + 1, out, errors);
            }
        } else if path.is_file() && has_allowed_ext(&path, kind.file_extensions()) {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(Found {
                kind,
                id: format!("{}:{}", kind.id_prefix(), rel_unix(&rel)),
                name,
                relative_path: rel,
                path,
            });
        }
    }
}

fn has_allowed_ext(path: &Path, allowed: &[&str]) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            allowed.iter().any(|a| *a == ext)
        }
        None => false,
    }
}

fn rel_unix(rel: &Path) -> String {
    rel.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    const WS_TOOLS: [ToolId; 3] = [ToolId::Codex, ToolId::Claude, ToolId::Cursor];

    #[test]
    fn discovers_resources_per_tool() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        // Codex skill/agent under .agents, Cursor rule + a workspace AGENTS.md.
        write(&ws.join(".agents/skills/dev/tdd/SKILL.md"), "# tdd");
        write(&ws.join(".agents/agents/coder.md"), "# coder");
        write(&ws.join(".cursor/rules/precise.mdc"), "> rule");
        write(&ws.join("AGENTS.md"), "# project agents");

        let inv = scan_workspace(ws, &WS_TOOLS);
        assert!(inv.errors.is_empty(), "{:?}", inv.errors);

        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"skill:dev/tdd"));
        assert!(ids.contains(&"agent:coder.md"));
        assert!(ids.contains(&"rule:precise.mdc"));
        assert!(ids.contains(&"rule:AGENTS.md"), "instruction file row: {ids:?}");

        // The Codex skill state targets the workspace .agents path.
        let skill_state = inv
            .states
            .iter()
            .find(|s| s.item_id == "skill:dev/tdd")
            .unwrap();
        assert_eq!(skill_state.tool, ToolId::Codex);
        assert_eq!(skill_state.state, LinkState::Enabled);
        assert!(skill_state.target_path.ends_with(".agents/skills/dev/tdd"));
    }

    #[test]
    fn same_skill_across_tools_dedupes_to_one_row_with_two_states() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".cursor/skills/shared/SKILL.md"), "# shared");
        write(&ws.join(".claude/skills/shared/SKILL.md"), "# shared");

        let inv = scan_workspace(ws, &WS_TOOLS);
        let rows = inv.items.iter().filter(|i| i.id == "skill:shared").count();
        assert_eq!(rows, 1, "deduped to a single row");

        let tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "skill:shared")
            .map(|s| s.tool)
            .collect();
        assert!(tools.contains(&ToolId::Cursor));
        assert!(tools.contains(&ToolId::Claude));
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn instruction_file_is_per_tool() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join("AGENTS.md"), "# codex");
        write(&ws.join("CLAUDE.md"), "# claude");

        let inv = scan_workspace(ws, &WS_TOOLS);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"rule:AGENTS.md"));
        assert!(ids.contains(&"rule:CLAUDE.md"));

        let agents = inv
            .states
            .iter()
            .find(|s| s.item_id == "rule:AGENTS.md")
            .unwrap();
        assert_eq!(agents.tool, ToolId::Codex);
        let claude = inv
            .states
            .iter()
            .find(|s| s.item_id == "rule:CLAUDE.md")
            .unwrap();
        assert_eq!(claude.tool, ToolId::Claude);
    }

    #[test]
    fn empty_workspace_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let inv = scan_workspace(dir.path(), &WS_TOOLS);
        assert!(inv.items.is_empty());
        assert!(inv.states.is_empty());
        assert!(inv.errors.is_empty());
    }

    #[test]
    fn skips_archived_skills() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".cursor/skills/live/SKILL.md"), "# live");
        write(&ws.join(".cursor/skills/__archived__/old/SKILL.md"), "# old");

        let inv = scan_workspace(ws, &WS_TOOLS);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["skill:live"]);
    }

    #[test]
    fn openclaw_is_skipped_as_workspace_tool() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".openclaw/skills/x/SKILL.md"), "# x");
        // OpenClaw's adapter is disabled in workspace scope, so even if passed
        // it contributes nothing.
        let inv = scan_workspace(ws, &[ToolId::Openclaw]);
        assert!(inv.items.is_empty());
    }
}
