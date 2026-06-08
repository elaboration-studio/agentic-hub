//! Read-only workspace inventory: walk a project's own per-tool directories
//! (`.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
//! `AGENTS.md`, `CLAUDE.md`) and report which agentic resources each tool
//! already has. The inverse of projection — this never writes. The result feeds
//! the manager matrix in read-only mode (only present resources are emitted, so
//! every cell shown is "enabled"). See `docs/tech/modules/workspace-inventory.md`.

use std::collections::{BTreeMap, HashSet};
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
        // Skill / agent dirs this tool honors. Cursor also reads the shared
        // `.agents/` standard dir (the same one Codex uses for skills), in
        // addition to its own `.cursor/` dirs — so resources dropped in
        // `.agents/` show up for Cursor too.
        let mut skill_dirs = vec![adapter.skills_path.clone()];
        let mut agent_dirs = vec![adapter.agents_path.clone()];
        if tool == ToolId::Cursor {
            skill_dirs.push(ws.join(".agents/skills"));
            agent_dirs.push(ws.join(".agents/agents"));
        }
        for dir in &skill_dirs {
            collect_skills(dir, &mut found, &mut errors);
        }
        // Codex subagents are TOML files under `.codex/agents`; Claude and
        // Cursor agents are markdown. Source: the per-tool agent dir resolves
        // via the adapter; only the file extension differs by format.
        let agent_exts: &[&str] = if tool == ToolId::Codex {
            &["toml"]
        } else {
            &["md"]
        };
        for dir in &agent_dirs {
            collect_files(
                dir,
                CapabilityKind::Agent,
                agent_exts,
                &mut found,
                &mut errors,
            );
        }
        // Codex/Claude rules live in their managed instruction block, not a
        // rules dir; only Cursor keeps per-file rules under `.cursor/rules`.
        if tool == ToolId::Cursor {
            collect_files(
                &adapter.rules_path,
                CapabilityKind::Rule,
                CapabilityKind::Rule.file_extensions(),
                &mut found,
                &mut errors,
            );
        }
        // Slash-command prompts: nested `.md` under the tool's commands dir
        // (`.cursor/commands`, `.claude/commands`, `.codex/prompts`). Read-only.
        if let Some(commands) = adapter.commands_path.as_ref() {
            collect_files(
                commands,
                CapabilityKind::Command,
                CapabilityKind::Command.file_extensions(),
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

        // A resource can surface from more than one of this tool's dirs (e.g.
        // Cursor reading both `.cursor/skills` and `.agents/skills`); emit one
        // state per id so the matrix shows a single cell per (tool, item).
        let mut seen: HashSet<String> = HashSet::new();
        for f in found {
            if !seen.insert(f.id.clone()) {
                continue;
            }
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

/// Walk a file-based directory (agents or Cursor rules) for `exts`; each
/// matching file is one resource. Extensions are passed explicitly because a
/// kind can use a different on-disk format per tool (Codex agents are `.toml`,
/// other agents are `.md`).
fn collect_files(
    base: &Path,
    kind: CapabilityKind,
    exts: &[&str],
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    if !base.is_dir() {
        return;
    }
    walk_files(base, base, kind, exts, 0, out, errors);
}

fn walk_files(
    base: &Path,
    dir: &Path,
    kind: CapabilityKind,
    exts: &[&str],
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
                walk_files(base, &path, kind, exts, depth + 1, out, errors);
            }
        } else if path.is_file() && has_allowed_ext(&path, exts) {
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
        // Codex skill under .agents, Codex subagent under .codex/agents (TOML),
        // Cursor rule + a workspace AGENTS.md.
        write(&ws.join(".agents/skills/dev/tdd/SKILL.md"), "# tdd");
        write(&ws.join(".codex/agents/coder.toml"), "name = \"coder\"");
        write(&ws.join(".cursor/rules/precise.mdc"), "> rule");
        write(&ws.join("AGENTS.md"), "# project agents");

        let inv = scan_workspace(ws, &WS_TOOLS);
        assert!(inv.errors.is_empty(), "{:?}", inv.errors);

        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"skill:dev/tdd"));
        assert!(ids.contains(&"agent:coder.toml"));
        assert!(ids.contains(&"rule:precise.mdc"));
        assert!(
            ids.contains(&"rule:AGENTS.md"),
            "instruction file row: {ids:?}"
        );

        // The Codex skill state targets the workspace .agents path.
        let skill_state = inv
            .states
            .iter()
            .find(|s| s.item_id == "skill:dev/tdd")
            .unwrap();
        assert_eq!(skill_state.tool, ToolId::Codex);
        assert_eq!(skill_state.state, LinkState::Enabled);
        assert!(skill_state.target_path.ends_with(".agents/skills/dev/tdd"));

        // The Codex subagent state targets `.codex/agents` (TOML format).
        let agent_state = inv
            .states
            .iter()
            .find(|s| s.item_id == "agent:coder.toml")
            .unwrap();
        assert_eq!(agent_state.tool, ToolId::Codex);
        assert!(agent_state
            .target_path
            .ends_with(".codex/agents/coder.toml"));
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
    fn cursor_also_reads_the_shared_agents_dir() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        // A skill + a markdown agent dropped only in the shared `.agents/` dir.
        write(
            &ws.join(".agents/skills/dev/web-design/adapt/SKILL.md"),
            "# adapt",
        );
        write(&ws.join(".agents/agents/coder.md"), "# coder");

        let inv = scan_workspace(ws, &WS_TOOLS);

        // Skills under `.agents/skills` are shared by Codex and Cursor; not
        // Claude (which scans only `.claude/skills`).
        let skill_tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "skill:dev/web-design/adapt")
            .map(|s| s.tool)
            .collect();
        assert!(skill_tools.contains(&ToolId::Codex), "{skill_tools:?}");
        assert!(skill_tools.contains(&ToolId::Cursor), "{skill_tools:?}");
        assert!(!skill_tools.contains(&ToolId::Claude), "{skill_tools:?}");

        // The markdown agent in `.agents/agents` is Cursor-only — Codex
        // subagents are TOML files under `.codex/agents`, so Codex does not
        // claim this one.
        let agent_tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "agent:coder.md")
            .map(|s| s.tool)
            .collect();
        assert!(agent_tools.contains(&ToolId::Cursor), "{agent_tools:?}");
        assert!(!agent_tools.contains(&ToolId::Codex), "{agent_tools:?}");
    }

    #[test]
    fn cursor_resource_in_both_dirs_emits_one_state() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        // Same skill id present under both `.cursor/skills` and `.agents/skills`.
        write(&ws.join(".cursor/skills/shared/SKILL.md"), "# shared");
        write(&ws.join(".agents/skills/shared/SKILL.md"), "# shared");

        let inv = scan_workspace(ws, &[ToolId::Cursor]);
        let cursor_states = inv
            .states
            .iter()
            .filter(|s| s.item_id == "skill:shared" && s.tool == ToolId::Cursor)
            .count();
        assert_eq!(cursor_states, 1, "deduped to one Cursor state");
    }

    #[test]
    fn cursor_skills_are_discovered_recursively_with_folder_name() {
        // Verified: Cursor walks the skills root recursively, so category
        // subfolders work for grouping; the skill name comes from the folder
        // that holds SKILL.md, not the category above it.
        // Source: https://cursor.com/help/customization/skills (2026).
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(
            &ws.join(".cursor/skills/shipping/deploy-staging/SKILL.md"),
            "# deploy",
        );

        let inv = scan_workspace(ws, &[ToolId::Cursor]);
        let item = inv
            .items
            .iter()
            .find(|i| i.id == "skill:shipping/deploy-staging")
            .expect("nested skill discovered");
        assert_eq!(item.name, "deploy-staging", "name is the leaf folder");
        assert!(inv
            .states
            .iter()
            .any(|s| s.item_id == "skill:shipping/deploy-staging" && s.tool == ToolId::Cursor));
    }

    #[test]
    fn cursor_rules_discovers_nested_mdc_files() {
        // Verified: Cursor project rules are `.mdc` files under `.cursor/rules`,
        // and may be organized into subfolders.
        // Source: https://cursor.com/docs/rules (2026).
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(
            &ws.join(".cursor/rules/general/precise.mdc"),
            "---\n---\n> rule",
        );

        let inv = scan_workspace(ws, &WS_TOOLS);
        let rule = inv
            .states
            .iter()
            .find(|s| s.item_id == "rule:general/precise.mdc")
            .expect("nested .mdc rule discovered");
        assert_eq!(rule.tool, ToolId::Cursor, "rules dir is Cursor-only");
    }

    #[test]
    fn codex_skills_in_dot_agents_subagents_in_dot_codex_agents() {
        // Verified: Codex reads repo skills from `.agents/skills` (CWD→repo
        // root) and project AGENTS.md as instructions, but subagents are TOML
        // files under `.codex/agents` — not `.agents/agents`.
        // Sources: developers.openai.com/codex/{skills,subagents,guides/agents-md}.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".agents/skills/dev/tdd/SKILL.md"), "# tdd");
        write(
            &ws.join(".codex/agents/reviewer.toml"),
            "name = \"reviewer\"",
        );
        // A markdown file in `.agents/agents` is NOT a Codex subagent.
        write(&ws.join(".agents/agents/stray.md"), "# stray");
        write(&ws.join("AGENTS.md"), "# project");

        let inv = scan_workspace(ws, &[ToolId::Codex]);
        let skill = inv
            .states
            .iter()
            .find(|s| s.item_id == "skill:dev/tdd")
            .expect("codex skill under .agents/skills");
        assert_eq!(skill.tool, ToolId::Codex);
        assert!(skill.target_path.ends_with(".agents/skills/dev/tdd"));

        assert!(
            inv.states
                .iter()
                .any(|s| s.item_id == "agent:reviewer.toml" && s.tool == ToolId::Codex),
            "codex subagent discovered under .codex/agents"
        );
        assert!(
            !inv.states.iter().any(|s| s.item_id == "agent:stray.md"),
            "markdown in .agents/agents is not a Codex subagent"
        );
        assert!(inv
            .states
            .iter()
            .any(|s| s.item_id == "rule:AGENTS.md" && s.tool == ToolId::Codex));
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

        // AGENTS.md is read by both Codex and Cursor; CLAUDE.md by Claude only.
        let agents_tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "rule:AGENTS.md")
            .map(|s| s.tool)
            .collect();
        assert!(agents_tools.contains(&ToolId::Codex), "{agents_tools:?}");
        assert!(agents_tools.contains(&ToolId::Cursor), "{agents_tools:?}");
        assert!(!agents_tools.contains(&ToolId::Claude), "{agents_tools:?}");

        let claude_tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "rule:CLAUDE.md")
            .map(|s| s.tool)
            .collect();
        assert_eq!(claude_tools, vec![ToolId::Claude]);
    }

    #[test]
    fn discovers_nested_commands_per_tool() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        // Cursor + Claude commands under their `commands` dirs; Codex under
        // `prompts`. All nested markdown.
        write(
            &ws.join(".cursor/commands/review/code-review.md"),
            "# review",
        );
        write(&ws.join(".claude/commands/git/commit.md"), "# commit");
        write(&ws.join(".codex/prompts/plan.md"), "# plan");

        let inv = scan_workspace(ws, &WS_TOOLS);
        assert!(inv.errors.is_empty(), "{:?}", inv.errors);

        let cursor_cmd = inv
            .states
            .iter()
            .find(|s| s.item_id == "command:review/code-review.md")
            .expect("cursor command discovered");
        assert_eq!(cursor_cmd.tool, ToolId::Cursor);
        assert!(cursor_cmd
            .target_path
            .ends_with(".cursor/commands/review/code-review.md"));

        assert!(inv
            .states
            .iter()
            .any(|s| s.item_id == "command:git/commit.md" && s.tool == ToolId::Claude));
        assert!(inv
            .states
            .iter()
            .any(|s| s.item_id == "command:plan.md" && s.tool == ToolId::Codex));
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
        write(
            &ws.join(".cursor/skills/__archived__/old/SKILL.md"),
            "# old",
        );

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
