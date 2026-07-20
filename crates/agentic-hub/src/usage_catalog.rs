//! Tool-aware global and repository skill catalog used by usage attribution.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use agentic_core::model::{CapabilityItem, CapabilityKind, CapabilityScope, SourceRef, ToolId};
use agentic_core::paths::expand_tilde;
use agentic_core::paths::tildify;
use agentic_core::settings::Settings;
use agentic_core::{
    api, hash_usage_correlation, scan_installed_tools, UsageEventInput, UsageStore,
};

use crate::usage_attribution::{path_is_capability_file, NormalizedBatch, NormalizedOccurrence};

const MAX_SKILL_DEPTH: usize = 16;

#[derive(Debug)]
struct CatalogEntry {
    item: CapabilityItem,
    scope: CapabilityScope,
    workspace_root: Option<String>,
    canonical_skill_file: PathBuf,
}

pub struct AttributedBatch {
    pub events: Vec<UsageEventInput>,
    pub catalog_items: Vec<CapabilityItem>,
}

pub fn attribute_batch(
    batch: NormalizedBatch,
    settings: &Settings,
    source_tool: &str,
) -> AttributedBatch {
    let default_workspace = unique_workspace_root(&batch.workspace_roots);
    let entries = build_catalog(settings, source_tool, &batch.workspace_roots);
    let catalog_items = entries.iter().map(|entry| entry.item.clone()).collect();
    let mut attributed: BTreeMap<String, UsageEventInput> = BTreeMap::new();

    for occurrence in batch.occurrences {
        let resolved = resolve_occurrence(&entries, source_tool, &occurrence);
        if resolved.is_none() && occurrence.requires_catalog_match {
            continue;
        }
        let mut event = occurrence.event;
        let identity = if let Some(entry) = resolved {
            event.capability_id = Some(entry.item.id.clone());
            event.capability_scope = entry.scope;
            event.workspace_root = entry.workspace_root.clone();
            event.capability_relative_path = Some(
                entry
                    .item
                    .relative_path
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            format!(
                "{}\0{}\0{}",
                entry.scope.as_str(),
                entry.workspace_root.as_deref().unwrap_or_default(),
                entry.item.id
            )
        } else {
            event.capability_scope = if default_workspace.is_some() {
                CapabilityScope::Workspace
            } else {
                CapabilityScope::Global
            };
            event.workspace_root = default_workspace.clone();
            format!(
                "unresolved\0{}\0{}",
                event.workspace_root.as_deref().unwrap_or_default(),
                event.skill_name.as_deref().unwrap_or_default()
            )
        };
        event.workspace = event.workspace_root.clone().or(event.workspace);
        event.invocation_key = Some(hash_usage_correlation(&format!(
            "{}\0{}\0{}",
            source_tool, occurrence.correlation_hash, identity
        )));
        if let Some(legacy) = event.dedupe_hash.take() {
            event.dedupe_hash = Some(hash_usage_correlation(&format!("{legacy}\0{identity}")));
        }
        let replace = attributed.get(&identity).map_or(true, |existing| {
            existing.attribution_rank < event.attribution_rank
        });
        if replace {
            attributed.insert(identity, event);
        }
    }

    AttributedBatch {
        events: attributed.into_values().collect(),
        catalog_items,
    }
}

pub fn reconcile_existing_usage(store: &UsageStore, settings: &Settings) -> Result<u32, String> {
    let references = store
        .unresolved_references()
        .map_err(|error| error.to_string())?;
    let mut updated = 0_u32;
    for reference in references {
        let Some(workspace_root) = reference.workspace_root.as_deref() else {
            continue;
        };
        let root = expand_tilde(workspace_root);
        if !root.is_dir() {
            continue;
        }
        let batch = NormalizedBatch {
            occurrences: vec![NormalizedOccurrence {
                event: UsageEventInput {
                    source_tool: reference.source_tool.clone(),
                    event_type: "PostSkillUse".to_string(),
                    skill_name: Some(reference.skill_name),
                    ..UsageEventInput::default()
                },
                exact_skill_path: None,
                requires_catalog_match: false,
                correlation_hash: String::new(),
            }],
            workspace_roots: vec![root],
        };
        let attributed = attribute_batch(batch, settings, &reference.source_tool);
        let Some(event) = attributed.events.first() else {
            continue;
        };
        let Some(capability_id) = event.capability_id.as_deref() else {
            continue;
        };
        if store
            .reconcile_resolution(
                &reference.dedupe_hash,
                capability_id,
                event.capability_scope,
                event.workspace_root.as_deref(),
                event.capability_relative_path.as_deref(),
            )
            .map_err(|error| error.to_string())?
        {
            updated = updated.saturating_add(1);
        }
    }
    Ok(updated)
}

fn build_catalog(settings: &Settings, source_tool: &str, roots: &[PathBuf]) -> Vec<CatalogEntry> {
    let mut entries = Vec::new();
    let scan = api::scan(settings);
    for item in scan.items.into_iter().filter(is_skill_or_agent) {
        if let Some(entry) = global_entry(item) {
            entries.push(entry);
        }
    }
    let installed = scan_installed_tools(settings);
    let installed_prefix = format!("installed::{source_tool}::");
    for item in installed
        .items
        .into_iter()
        .filter(|item| item.id.starts_with(&installed_prefix) && is_skill_or_agent(item))
    {
        if let Some(entry) = global_entry(item) {
            entries.push(entry);
        }
    }
    for active_root in canonical_active_roots(roots) {
        let repository_root =
            find_repository_root(&active_root).unwrap_or_else(|| active_root.clone());
        collect_repository_entries(source_tool, &repository_root, &active_root, &mut entries);
    }
    dedupe_entries(entries)
}

fn global_entry(item: CapabilityItem) -> Option<CatalogEntry> {
    let marker = marker_path(&item);
    let canonical_skill_file = marker.canonicalize().ok()?;
    Some(CatalogEntry {
        item,
        scope: CapabilityScope::Global,
        workspace_root: None,
        canonical_skill_file,
    })
}

fn collect_repository_entries(
    source_tool: &str,
    repository_root: &Path,
    active_root: &Path,
    out: &mut Vec<CatalogEntry>,
) {
    let Some(tool) = tool_id(source_tool) else {
        return;
    };
    for skill_root in documented_skill_roots(tool, repository_root, active_root) {
        collect_skill_root(repository_root, &skill_root, &skill_root, 0, out);
    }
    for agent_root in documented_agent_roots(tool, repository_root, active_root) {
        collect_agent_root(repository_root, &agent_root, &agent_root, 0, out);
    }
}

fn documented_skill_roots(
    tool: ToolId,
    repository_root: &Path,
    active_root: &Path,
) -> Vec<PathBuf> {
    let ancestors: Vec<&Path> = active_root
        .ancestors()
        .take_while(|ancestor| ancestor.starts_with(repository_root))
        .collect();
    match tool {
        ToolId::Cursor => [
            ".agents/skills",
            ".cursor/skills",
            ".claude/skills",
            ".codex/skills",
        ]
        .into_iter()
        .map(|relative| repository_root.join(relative))
        .collect(),
        ToolId::Claude => ancestors
            .into_iter()
            .map(|ancestor| ancestor.join(".claude/skills"))
            .collect(),
        ToolId::Codex => ancestors
            .into_iter()
            .map(|ancestor| ancestor.join(".agents/skills"))
            .collect(),
        _ => Vec::new(),
    }
}

fn documented_agent_roots(
    tool: ToolId,
    repository_root: &Path,
    active_root: &Path,
) -> Vec<PathBuf> {
    let ancestors: Vec<&Path> = active_root
        .ancestors()
        .take_while(|ancestor| ancestor.starts_with(repository_root))
        .collect();
    match tool {
        ToolId::Cursor => [".cursor/agents", ".agents/agents"]
            .into_iter()
            .map(|relative| repository_root.join(relative))
            .collect(),
        ToolId::Claude => ancestors
            .into_iter()
            .map(|ancestor| ancestor.join(".claude/agents"))
            .collect(),
        ToolId::Codex => ancestors
            .into_iter()
            .flat_map(|ancestor| {
                [
                    ancestor.join(".codex/agents"),
                    ancestor.join(".agents/agents"),
                ]
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn collect_skill_root(
    repository_root: &Path,
    skill_root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<CatalogEntry>,
) {
    if depth > MAX_SKILL_DEPTH || !dir.is_dir() {
        return;
    }
    let Ok(canonical_root) = skill_root.canonicalize() else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || entry.file_name() == "__archived__" {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if let Ok(canonical_skill_file) = skill_file.canonicalize() {
            if canonical_skill_file.starts_with(&canonical_root) {
                if let Some(item) = repository_item(repository_root, skill_root, &path) {
                    out.push(CatalogEntry {
                        item,
                        scope: CapabilityScope::Workspace,
                        workspace_root: Some(tildify(repository_root)),
                        canonical_skill_file,
                    });
                }
            }
        }
        collect_skill_root(repository_root, skill_root, &path, depth + 1, out);
    }
}

fn collect_agent_root(
    repository_root: &Path,
    agent_root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<CatalogEntry>,
) {
    if depth > MAX_SKILL_DEPTH || !dir.is_dir() {
        return;
    }
    let Ok(canonical_root) = agent_root.canonicalize() else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "__archived__" {
                collect_agent_root(repository_root, agent_root, &path, depth + 1, out);
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        // Cursor/Claude agents are markdown; Codex subagents are TOML.
        if !matches!(ext, "md" | "toml") {
            continue;
        }
        let Ok(canonical_agent_file) = path.canonicalize() else {
            continue;
        };
        if !canonical_agent_file.starts_with(&canonical_root) {
            continue;
        }
        if let Some(item) = repository_agent_item(repository_root, agent_root, &path) {
            out.push(CatalogEntry {
                item,
                scope: CapabilityScope::Workspace,
                workspace_root: Some(tildify(repository_root)),
                canonical_skill_file: canonical_agent_file,
            });
        }
    }
}

fn repository_item(
    repository_root: &Path,
    skill_root: &Path,
    skill_dir: &Path,
) -> Option<CapabilityItem> {
    let relative_path = skill_dir.strip_prefix(skill_root).ok()?.to_path_buf();
    let name = relative_path.file_name()?.to_string_lossy().into_owned();
    Some(CapabilityItem {
        id: format!(
            "skill:{}",
            relative_path.to_string_lossy().replace('\\', "/")
        ),
        kind: CapabilityKind::Skill,
        name,
        source_path: skill_dir.to_path_buf(),
        relative_path,
        source_id: "workspace".to_string(),
        source_label: "Workspace".to_string(),
        source: SourceRef {
            rel_home: tildify(repository_root),
            folder: repository_root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        },
        valid: true,
        validation_errors: Vec::new(),
    })
}

fn repository_agent_item(
    repository_root: &Path,
    agent_root: &Path,
    agent_file: &Path,
) -> Option<CapabilityItem> {
    let relative_path = agent_file.strip_prefix(agent_root).ok()?.to_path_buf();
    let name = agent_file.file_stem()?.to_string_lossy().into_owned();
    if name.is_empty() {
        return None;
    }
    Some(CapabilityItem {
        id: format!(
            "agent:{}",
            relative_path.to_string_lossy().replace('\\', "/")
        ),
        kind: CapabilityKind::Agent,
        name,
        source_path: agent_file.to_path_buf(),
        relative_path,
        source_id: "workspace".to_string(),
        source_label: "Workspace".to_string(),
        source: SourceRef {
            rel_home: tildify(repository_root),
            folder: repository_root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        },
        valid: true,
        validation_errors: Vec::new(),
    })
}

fn resolve_occurrence<'a>(
    entries: &'a [CatalogEntry],
    source_tool: &str,
    occurrence: &NormalizedOccurrence,
) -> Option<&'a CatalogEntry> {
    if let Some(path) = occurrence.exact_skill_path.as_deref() {
        if !path.is_absolute() || !path_is_capability_file(path) {
            return None;
        }
        let canonical = path.canonicalize().ok()?;
        return unique(
            entries
                .iter()
                .filter(|entry| entry.canonical_skill_file == canonical),
        );
    }
    let name = occurrence.event.skill_name.as_deref()?;
    let locals: Vec<&CatalogEntry> = entries
        .iter()
        .filter(|entry| entry.scope == CapabilityScope::Workspace && entry.item.name == name)
        .collect();
    let globals: Vec<&CatalogEntry> = entries
        .iter()
        .filter(|entry| entry.scope == CapabilityScope::Global && entry.item.name == name)
        .collect();
    match source_tool {
        "claude" => unique(globals.into_iter()).or_else(|| unique(locals.into_iter())),
        // Cursor and Codex: workspace-unique first, then global-unique.
        _ => unique(locals.into_iter()).or_else(|| unique(globals.into_iter())),
    }
}

fn unique<'a>(matches: impl Iterator<Item = &'a CatalogEntry>) -> Option<&'a CatalogEntry> {
    let mut canonical = HashSet::new();
    let mut unique = Vec::new();
    for entry in matches {
        if canonical.insert(entry.canonical_skill_file.clone()) {
            unique.push(entry);
        }
    }
    (unique.len() == 1).then(|| unique[0])
}

fn canonical_active_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    roots
        .iter()
        .filter(|root| root.is_absolute())
        .filter_map(|root| root.canonicalize().ok())
        .filter(|root| root.is_dir())
        .filter(|root| seen.insert(root.clone()))
        .collect()
}

fn unique_workspace_root(roots: &[PathBuf]) -> Option<String> {
    let repositories: HashSet<String> = canonical_active_roots(roots)
        .into_iter()
        .map(|active_root| {
            let repository_root = find_repository_root(&active_root).unwrap_or(active_root);
            tildify(&repository_root)
        })
        .collect();
    if repositories.len() != 1 {
        return None;
    }
    repositories.into_iter().next()
}

fn find_repository_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

fn marker_path(item: &CapabilityItem) -> PathBuf {
    if item.kind == CapabilityKind::Skill {
        item.source_path.join("SKILL.md")
    } else {
        item.source_path.clone()
    }
}

fn is_skill_or_agent(item: &CapabilityItem) -> bool {
    matches!(item.kind, CapabilityKind::Skill | CapabilityKind::Agent)
}

fn tool_id(value: &str) -> Option<ToolId> {
    match value {
        "cursor" => Some(ToolId::Cursor),
        "claude" => Some(ToolId::Claude),
        "codex" => Some(ToolId::Codex),
        _ => None,
    }
}

fn dedupe_entries(entries: Vec<CatalogEntry>) -> Vec<CatalogEntry> {
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter(|entry| {
            seen.insert((
                entry.scope,
                entry.workspace_root.clone(),
                entry.canonical_skill_file.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
