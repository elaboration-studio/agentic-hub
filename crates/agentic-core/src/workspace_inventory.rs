//! Read-only workspace inventory: walk a project's own per-tool directories
//! (`.cursor/skills`, `.claude/skills`, `.agents/skills`, `.cursor/rules`,
//! `AGENTS.md`, `CLAUDE.md`) and report which agentic resources each tool
//! already has. The inverse of projection — this never writes. The result feeds
//! the manager matrix in read-only mode (only present resources are emitted, so
//! every cell shown is "enabled"). See `docs/tech/modules/workspace-inventory.md`.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapter_registry::ResolvedAdapter;
use crate::adapter_registry::{create_workspace_adapter, resolve};
use crate::model::{
    CapabilityItem, CapabilityKind, LinkState, ScanError, SourceRef, ToolCapabilityState, ToolId,
};
use crate::paths::tildify;
use crate::settings::Settings;
use crate::skill_lock::read_local_lock;

/// Bounded walk depth; mirrors the shared-root scanner guard against cycles.
const MAX_DEPTH: usize = 16;

/// Reserved folder name holding old versions of files. Never scanned.
const ARCHIVED: &str = "__archived__";

/// Synthetic source id/label for every workspace-discovered item. Workspace
/// scope has a single implicit source (the project itself).
const WORKSPACE_SOURCE_ID: &str = "workspace";
const WORKSPACE_SOURCE_LABEL: &str = "Workspace";

const INSTALLED_ID_PREFIX: &str = "installed";

/// A skill in this workspace that the skills.sh CLI manages, matched from the
/// project's `skills-lock.json`. Carries what the hub needs to offer a one-click
/// `npx skills update`: the inventory item it annotates and the install source.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedSkill {
    /// The inventory item id this annotates (`skill:<rel>`, pre-namespacing).
    pub item_id: String,
    /// The lock key — the skill's install name, passed to `skills update`.
    pub name: String,
    /// The `owner/repo` (or other transport) source it was installed from.
    pub source: String,
    /// Transport hint from the lock: `github`, `local`, etc.
    pub source_type: String,
}

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
    /// Skill items the skills.sh CLI manages (from `skills-lock.json`), so the
    /// UI can mark them and offer `npx skills update`. Empty when no lock.
    pub locked_skills: Vec<LockedSkill>,
}

/// A read-only snapshot of resources already installed in enabled tools'
/// global folders, outside the configured Agentic Hub source roots.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledToolInventory {
    pub items: Vec<CapabilityItem>,
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
        if tool == ToolId::Copilot {
            skill_dirs.push(ws.join(".agents/skills"));
        }
        if tool == ToolId::Antigravity {
            skill_dirs.push(ws.join(".agent/skills"));
        }
        for dir in &skill_dirs {
            collect_skills(dir, &mut found, &mut errors);
        }
        let agent_exts: &[&str] = if tool == ToolId::Codex {
            &["toml"]
        } else {
            &["md"]
        };
        for dir in &agent_dirs {
            if tool == ToolId::Copilot {
                collect_copilot_agents(dir, &mut found, &mut errors);
            } else {
                collect_files(
                    dir,
                    CapabilityKind::Agent,
                    agent_exts,
                    &mut found,
                    &mut errors,
                );
            }
        }
        if tool == ToolId::Cursor || tool == ToolId::Kiro {
            collect_files(
                &adapter.rules_path,
                CapabilityKind::Rule,
                CapabilityKind::Rule.file_extensions(),
                &mut found,
                &mut errors,
            );
        }
        if tool == ToolId::Copilot {
            collect_copilot_rules(&adapter.rules_path, &mut found, &mut errors);
        }
        if tool == ToolId::Antigravity {
            collect_files(
                &adapter.rules_path,
                CapabilityKind::Rule,
                CapabilityKind::Rule.file_extensions(),
                &mut found,
                &mut errors,
            );
            let legacy_rules = ws.join(".agent/rules");
            collect_files(
                &legacy_rules,
                CapabilityKind::Rule,
                CapabilityKind::Rule.file_extensions(),
                &mut found,
                &mut errors,
            );
        }
        if tool == ToolId::Kiro {
            collect_kiro_hooks(&adapter, &mut found, &mut errors);
        }
        if tool == ToolId::Copilot {
            collect_copilot_hooks(&adapter, &mut found, &mut errors);
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
        for instructions in instruction_scan_paths(tool, ws, &adapter) {
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

    let items: Vec<CapabilityItem> = by_id.into_values().collect();
    let locked_skills = mark_locked_skills(ws, &items);

    WorkspaceInventory {
        items,
        states,
        errors,
        locked_skills,
    }
}

/// Scan enabled global tool folders for unmanaged installed resources.
/// Read-only: no filesystem writes, and missing tool dirs are not errors.
pub fn scan_installed_tools(settings: &Settings) -> InstalledToolInventory {
    let mut by_id: BTreeMap<String, CapabilityItem> = BTreeMap::new();
    let mut states: Vec<ToolCapabilityState> = Vec::new();
    let mut errors: Vec<ScanError> = Vec::new();

    for tool in ToolId::ALL {
        let adapter = resolve(settings, tool);
        if !adapter.enabled {
            continue;
        }

        let mut found = Vec::new();
        collect_installed_for_tool(tool, &adapter, &mut found, &mut errors);

        let source_id = format!("installed:{}", tool.as_str());
        let source_label = tool_label(tool).to_string();
        let source = installed_source_ref(&adapter);
        let mut seen: HashSet<String> = HashSet::new();

        for f in found {
            let item_id = format!("{INSTALLED_ID_PREFIX}::{}::{}", tool.as_str(), f.id);
            if !seen.insert(item_id.clone()) {
                continue;
            }
            states.push(ToolCapabilityState {
                tool,
                item_id: item_id.clone(),
                target_path: f.path.clone(),
                state: LinkState::Enabled,
                current_link_target: None,
            });
            by_id
                .entry(item_id.clone())
                .or_insert_with(|| CapabilityItem {
                    id: item_id,
                    kind: f.kind,
                    name: f.name,
                    source_path: f.path,
                    relative_path: f.relative_path,
                    source_id: source_id.clone(),
                    source_label: source_label.clone(),
                    source: source.clone(),
                    valid: true,
                    validation_errors: Vec::new(),
                });
        }
    }

    InstalledToolInventory {
        items: by_id.into_values().collect(),
        states,
        errors,
    }
}

fn collect_installed_for_tool(
    tool: ToolId,
    adapter: &ResolvedAdapter,
    found: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    collect_skills(&adapter.skills_path, found, errors);

    match tool {
        ToolId::Codex => collect_files(
            &adapter.agents_path,
            CapabilityKind::Agent,
            &["toml"],
            found,
            errors,
        ),
        ToolId::Copilot => collect_copilot_agents(&adapter.agents_path, found, errors),
        ToolId::Antigravity => {}
        _ => collect_files(
            &adapter.agents_path,
            CapabilityKind::Agent,
            CapabilityKind::Agent.file_extensions(),
            found,
            errors,
        ),
    }

    match tool {
        ToolId::Copilot => collect_copilot_rules(&adapter.rules_path, found, errors),
        ToolId::Codex | ToolId::Openclaw => {}
        _ => collect_files(
            &adapter.rules_path,
            CapabilityKind::Rule,
            CapabilityKind::Rule.file_extensions(),
            found,
            errors,
        ),
    }

    match tool {
        ToolId::Kiro => collect_kiro_hooks(adapter, found, errors),
        ToolId::Copilot => collect_copilot_hooks(adapter, found, errors),
        _ => {}
    }

    if let Some(commands) = adapter.commands_path.as_ref() {
        collect_files(
            commands,
            CapabilityKind::Command,
            CapabilityKind::Command.file_extensions(),
            found,
            errors,
        );
    }

    if let Some(instructions) = adapter.instructions_path.as_ref() {
        if instructions.is_file() {
            if let Some(file_name) = instructions.file_name().and_then(|s| s.to_str()) {
                found.push(Found {
                    kind: CapabilityKind::Rule,
                    id: format!("{}:{file_name}", CapabilityKind::Rule.id_prefix()),
                    name: file_name.to_string(),
                    relative_path: PathBuf::from(file_name),
                    path: instructions.clone(),
                });
            }
        }
    }
}

fn tool_label(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Codex => "Codex",
        ToolId::Claude => "Claude",
        ToolId::Cursor => "Cursor",
        ToolId::Openclaw => "OpenClaw",
        ToolId::Openstandard => "OpenStandard",
        ToolId::Kiro => "Kiro",
        ToolId::Copilot => "Copilot",
        ToolId::Antigravity => "Antigravity",
    }
}

fn installed_source_ref(adapter: &ResolvedAdapter) -> SourceRef {
    let root = adapter
        .skills_path
        .parent()
        .unwrap_or(adapter.skills_path.as_path());
    let folder = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    SourceRef {
        rel_home: tildify(root),
        folder,
    }
}

/// Match inventory skill items against the project's `skills-lock.json` by their
/// install name (the skill's leaf folder). Tolerant: no lock file → empty.
fn mark_locked_skills(ws: &Path, items: &[CapabilityItem]) -> Vec<LockedSkill> {
    let Some(lock) = read_local_lock(ws) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|it| it.kind == CapabilityKind::Skill)
        .filter_map(|it| {
            lock.skills.get(&it.name).map(|entry| LockedSkill {
                item_id: it.id.clone(),
                name: it.name.clone(),
                source: entry.source.clone(),
                source_type: entry.source_type.clone(),
            })
        })
        .collect()
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

/// Inventory Kiro hook JSON files under `.kiro/hooks/*.json`.
fn collect_kiro_hooks(
    adapter: &ResolvedAdapter,
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    let Some(dir) = adapter.hooks_dir.as_ref() else {
        return;
    };
    if !dir.is_dir() {
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
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|v| {
                v.get("hooks")
                    .and_then(|h| h.as_array())
                    .and_then(|a| a.first())
                    .and_then(|e| e.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| stem.clone());
        let rel = path.strip_prefix(dir).unwrap_or(&path).to_path_buf();
        out.push(Found {
            kind: CapabilityKind::Hook,
            id: format!("{}:{}", CapabilityKind::Hook.id_prefix(), rel_unix(&rel)),
            name,
            relative_path: rel,
            path,
        });
    }
}

/// Copilot custom agents use the `*.agent.md` suffix.
fn collect_copilot_agents(base: &Path, out: &mut Vec<Found>, errors: &mut Vec<ScanError>) {
    if !base.is_dir() {
        return;
    }
    walk_copilot_agents(base, base, 0, out, errors);
}

fn walk_copilot_agents(
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
        if path.is_dir() {
            if entry.file_name() != std::ffi::OsStr::new(ARCHIVED) {
                walk_copilot_agents(base, &path, depth + 1, out, errors);
            }
        } else if path.is_file() && is_copilot_agent(&path) {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".agent.md"))
                .unwrap_or("")
                .to_string();
            out.push(Found {
                kind: CapabilityKind::Agent,
                id: format!("{}:{}", CapabilityKind::Agent.id_prefix(), rel_unix(&rel)),
                name,
                relative_path: rel,
                path,
            });
        }
    }
}

fn is_copilot_agent(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.ends_with(".agent.md"))
}

fn collect_copilot_rules(base: &Path, out: &mut Vec<Found>, errors: &mut Vec<ScanError>) {
    if !base.is_dir() {
        return;
    }
    walk_copilot_rules(base, base, 0, out, errors);
}

fn walk_copilot_rules(
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
        if path.is_dir() {
            if entry.file_name() != std::ffi::OsStr::new(ARCHIVED) {
                walk_copilot_rules(base, &path, depth + 1, out, errors);
            }
        } else if path.is_file() && is_copilot_instruction(&path) {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".instructions.md"))
                .unwrap_or("")
                .to_string();
            out.push(Found {
                kind: CapabilityKind::Rule,
                id: format!("{}:{}", CapabilityKind::Rule.id_prefix(), rel_unix(&rel)),
                name,
                relative_path: rel,
                path,
            });
        }
    }
}

fn is_copilot_instruction(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.ends_with(".instructions.md"))
}

/// Inventory Copilot hook JSON files under `.github/hooks/*.json`.
fn collect_copilot_hooks(
    adapter: &ResolvedAdapter,
    out: &mut Vec<Found>,
    errors: &mut Vec<ScanError>,
) {
    let Some(dir) = adapter.hooks_dir.as_ref() else {
        return;
    };
    if !dir.is_dir() {
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
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let rel = path.strip_prefix(dir).unwrap_or(&path).to_path_buf();
        out.push(Found {
            kind: CapabilityKind::Hook,
            id: format!("{}:{}", CapabilityKind::Hook.id_prefix(), rel_unix(&rel)),
            name: stem,
            relative_path: rel,
            path,
        });
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

/// Instruction files a tool reads for always-on steering. Most tools have one
/// path via `instructions_path`; Copilot and Antigravity also honor additional
/// AGENTS.md locations per their 2026 docs.
fn instruction_scan_paths(tool: ToolId, ws: &Path, adapter: &ResolvedAdapter) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = adapter.instructions_path.as_ref() {
        paths.push(p.clone());
    }
    match tool {
        ToolId::Copilot => {
            let root = ws.join("AGENTS.md");
            if !paths.iter().any(|p| p == &root) {
                paths.push(root);
            }
        }
        ToolId::Antigravity => {
            let nested = ws.join(".agents/AGENTS.md");
            if !paths.iter().any(|p| p == &nested) {
                paths.push(nested);
            }
        }
        _ => {}
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    const WS_TOOLS: [ToolId; 6] = [
        ToolId::Codex,
        ToolId::Claude,
        ToolId::Cursor,
        ToolId::Kiro,
        ToolId::Copilot,
        ToolId::Antigravity,
    ];

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
    fn discovers_kiro_steering_agents_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".kiro/steering/api-standards.md"), "# api");
        write(
            &ws.join(".kiro/agents/reviewer.md"),
            "---\nname: reviewer\n---\n# review",
        );
        write(
            &ws.join(".kiro/hooks/fmt.json"),
            r#"{"version":"v1","hooks":[{"name":"fmt","trigger":"Stop"}]}"#,
        );

        let inv = scan_workspace(ws, &[ToolId::Kiro]);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"rule:api-standards.md"));
        assert!(ids.contains(&"agent:reviewer.md"));
        assert!(ids.contains(&"hook:fmt.json"));
        assert!(inv.states.iter().all(|s| s.tool == ToolId::Kiro));
    }

    #[test]
    fn discovers_copilot_copilot_instructions_and_root_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".github/copilot-instructions.md"), "# repo");
        write(&ws.join("AGENTS.md"), "# agents standard");

        let inv = scan_workspace(ws, &[ToolId::Copilot]);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"rule:copilot-instructions.md"));
        assert!(ids.contains(&"rule:AGENTS.md"));
        assert_eq!(
            inv.states
                .iter()
                .filter(|s| s.tool == ToolId::Copilot && s.item_id.starts_with("rule:"))
                .count(),
            2
        );
    }

    #[test]
    fn discovers_antigravity_dot_agents_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".agents/AGENTS.md"), "# nested agents");

        let inv = scan_workspace(ws, &[ToolId::Antigravity]);
        assert!(inv
            .states
            .iter()
            .any(|s| s.item_id == "rule:AGENTS.md" && s.tool == ToolId::Antigravity));
    }

    #[test]
    fn discovers_copilot_agent_md_and_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(
            &ws.join(".github/agents/security-auditor.agent.md"),
            "# security",
        );
        write(
            &ws.join(".github/instructions/team/style.instructions.md"),
            "# style",
        );
        write(
            &ws.join(".github/hooks/fmt.json"),
            r#"{"version":1,"hooks":{"stop":[{"type":"command","bash":"echo"}]}}"#,
        );

        let inv = scan_workspace(ws, &[ToolId::Copilot]);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"agent:security-auditor.agent.md"));
        assert!(ids.contains(&"rule:team/style.instructions.md"));
        assert!(ids.contains(&"hook:fmt.json"));
    }

    #[test]
    fn discovers_antigravity_agents_skills_and_rules() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(&ws.join(".agents/skills/dev/tdd/SKILL.md"), "# tdd");
        write(&ws.join(".agents/rules/typescript.md"), "# ts");
        write(&ws.join("AGENTS.md"), "# project");

        let inv = scan_workspace(ws, &[ToolId::Antigravity]);
        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"skill:dev/tdd"));
        assert!(ids.contains(&"rule:typescript.md"));
        assert!(ids.contains(&"rule:AGENTS.md"));
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

        // AGENTS.md is read by Codex, Cursor, Kiro, Copilot, and Antigravity;
        // CLAUDE.md by Claude only.
        let agents_tools: Vec<ToolId> = inv
            .states
            .iter()
            .filter(|s| s.item_id == "rule:AGENTS.md")
            .map(|s| s.tool)
            .collect();
        assert!(agents_tools.contains(&ToolId::Codex), "{agents_tools:?}");
        assert!(agents_tools.contains(&ToolId::Cursor), "{agents_tools:?}");
        assert!(agents_tools.contains(&ToolId::Kiro), "{agents_tools:?}");
        assert!(agents_tools.contains(&ToolId::Copilot), "{agents_tools:?}");
        assert!(
            agents_tools.contains(&ToolId::Antigravity),
            "{agents_tools:?}"
        );
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
    fn marks_skills_present_in_the_project_lock() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        // Two installed skills; only one is recorded in skills-lock.json.
        write(
            &ws.join(".agents/skills/rust-best-practices/SKILL.md"),
            "# rust",
        );
        write(&ws.join(".agents/skills/handwritten/SKILL.md"), "# hand");
        write(
            &ws.join("skills-lock.json"),
            r#"{"version":1,"skills":{"rust-best-practices":{"source":"apollographql/skills","sourceType":"github"}}}"#,
        );

        let inv = scan_workspace(ws, &WS_TOOLS);
        assert_eq!(inv.locked_skills.len(), 1, "only the locked one is marked");
        let locked = &inv.locked_skills[0];
        assert_eq!(locked.item_id, "skill:rust-best-practices");
        assert_eq!(locked.name, "rust-best-practices");
        assert_eq!(locked.source, "apollographql/skills");
        assert_eq!(locked.source_type, "github");
    }

    #[test]
    fn no_lock_file_marks_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        write(
            &ws.join(".agents/skills/rust-best-practices/SKILL.md"),
            "# rust",
        );
        let inv = scan_workspace(ws, &WS_TOOLS);
        assert!(inv.locked_skills.is_empty());
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

    #[test]
    fn installed_inventory_discovers_enabled_tool_resources() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        settings.tools.kiro.enabled = true;
        settings.tools.copilot.enabled = true;

        write(&tools.path().join("codex/skills/tdd/SKILL.md"), "# tdd");
        write(
            &tools.path().join("codex/agents/reviewer.toml"),
            "name = \"reviewer\"",
        );
        write(&tools.path().join("kiro/steering/api.md"), "# api");
        write(
            &tools.path().join("kiro/hooks/fmt.json"),
            r#"{"hooks":[{"name":"fmt"}]}"#,
        );
        write(
            &tools
                .path()
                .join("copilot/agents/security-auditor.agent.md"),
            "# security",
        );

        let inv = scan_installed_tools(&settings);

        let ids: Vec<&str> = inv.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"installed::codex::skill:tdd"));
        assert!(ids.contains(&"installed::codex::agent:reviewer.toml"));
        assert!(ids.contains(&"installed::kiro::rule:api.md"));
        assert!(ids.contains(&"installed::kiro::hook:fmt.json"));
        assert!(ids.contains(&"installed::copilot::agent:security-auditor.agent.md"));
        assert!(inv
            .items
            .iter()
            .any(|i| i.source_id == "installed:kiro" && i.source_label == "Kiro"));
    }

    #[test]
    fn installed_inventory_skips_disabled_tools_and_archived_resources() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let mut settings = Settings::sandboxed(root.path(), tools.path());
        settings.tools.kiro.enabled = false;

        write(&tools.path().join("kiro/skills/disabled/SKILL.md"), "# off");
        write(
            &tools.path().join("codex/skills/__archived__/old/SKILL.md"),
            "# old",
        );

        let inv = scan_installed_tools(&settings);

        assert!(inv.items.is_empty());
        assert!(inv.states.is_empty());
        assert!(inv.errors.is_empty());
    }
}
