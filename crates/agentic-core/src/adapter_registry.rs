//! Per-tool adapter resolution. The single owner of the basename-vs-relative-path
//! decision (layout), the projection mode per `(tool, kind)`, and target-path
//! resolution. See `docs/tech/reference/tool-adapter-matrix.md`.

use std::path::PathBuf;

use crate::model::{CapabilityItem, CapabilityKind, ToolId};
use crate::settings::{Settings, ToolSettings};

/// How a kind's relative path maps onto the tool's target directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Collapse to basename (Claude's non-recursive loader).
    Flat,
    /// Preserve nested folder structure.
    Nested,
}

/// The mechanism used to project a `(tool, kind)` onto disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    LinkSync,
    FileSync,
    MarkdownSectionSync,
    JsonSection,
}

/// A tool's resolved, canonical-path adapter for global scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAdapter {
    pub tool_id: ToolId,
    pub enabled: bool,
    pub skills_path: PathBuf,
    pub agents_path: PathBuf,
    pub rules_path: PathBuf,
    pub instructions_path: Option<PathBuf>,
    pub hooks_enabled: bool,
    pub hooks_file: Option<PathBuf>,
}

fn tool_settings(settings: &Settings, tool: ToolId) -> &ToolSettings {
    match tool {
        ToolId::Codex => &settings.tools.codex,
        ToolId::Claude => &settings.tools.claude,
        ToolId::Cursor => &settings.tools.cursor,
        ToolId::Openclaw => &settings.tools.openclaw,
    }
}

/// Resolve one tool's adapter from settings.
pub fn resolve(settings: &Settings, tool: ToolId) -> ResolvedAdapter {
    let ts = tool_settings(settings, tool);
    ResolvedAdapter {
        tool_id: tool,
        enabled: ts.enabled,
        skills_path: ts.skills_path.clone(),
        agents_path: ts.agents_path.clone(),
        rules_path: ts.rules_path.clone(),
        instructions_path: ts.instructions_path.clone(),
        hooks_enabled: ts.hooks_enabled,
        hooks_file: ts.hooks_file.clone(),
    }
}

/// Resolve all four tool adapters.
pub fn resolve_all(settings: &Settings) -> Vec<ResolvedAdapter> {
    ToolId::ALL.iter().map(|&t| resolve(settings, t)).collect()
}

impl ResolvedAdapter {
    /// Layout for a kind. Claude skills/agents are flat; everything else nested.
    pub fn layout_for(&self, kind: CapabilityKind) -> Layout {
        match (self.tool_id, kind) {
            (ToolId::Claude, CapabilityKind::Skill | CapabilityKind::Agent) => Layout::Flat,
            _ => Layout::Nested,
        }
    }

    /// Projection mode for a `(tool, kind)` in global scope. `None` means the
    /// combination is unsupported (OpenClaw hooks).
    pub fn projection_mode_for(&self, kind: CapabilityKind) -> Option<ProjectionMode> {
        use CapabilityKind::{Agent, Hook, Rule, Skill};
        use ProjectionMode::{FileSync, JsonSection, LinkSync, MarkdownSectionSync};
        match (self.tool_id, kind) {
            (ToolId::Openclaw, Hook) => None,
            (_, Hook) => Some(JsonSection),
            (ToolId::Cursor, Agent) => Some(FileSync),
            (_, Skill | Agent) => Some(LinkSync),
            (ToolId::Cursor, Rule) => Some(LinkSync),
            (_, Rule) => Some(MarkdownSectionSync),
        }
    }

    /// Whether this `(tool, kind)` projects as a managed copy (Cursor agents).
    pub fn uses_managed_copy(&self, kind: CapabilityKind) -> bool {
        matches!(
            self.projection_mode_for(kind),
            Some(ProjectionMode::FileSync)
        )
    }

    /// The per-item target path for symlink/managed-copy kinds. `None` for hooks
    /// (which target the tool's single `hooks_file`, routed through `hook_sync`).
    pub fn target_path_for(&self, item: &CapabilityItem) -> Option<PathBuf> {
        let base = match item.kind {
            CapabilityKind::Skill => &self.skills_path,
            CapabilityKind::Agent => &self.agents_path,
            CapabilityKind::Rule => &self.rules_path,
            CapabilityKind::Hook => return None,
        };
        let rel = match self.layout_for(item.kind) {
            Layout::Nested => item.relative_path.clone(),
            Layout::Flat => PathBuf::from(item.relative_path.file_name()?),
        };
        Some(base.join(rel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn item(kind: CapabilityKind, rel: &str) -> CapabilityItem {
        CapabilityItem {
            id: format!("{}:{}", kind.id_prefix(), rel),
            kind,
            name: rel.to_string(),
            source_path: PathBuf::from(format!("/src/{rel}")),
            relative_path: PathBuf::from(rel),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            valid: true,
            validation_errors: vec![],
        }
    }

    #[test]
    fn claude_skill_is_flat_others_nested() {
        let settings = Settings::default();
        let claude = resolve(&settings, ToolId::Claude);
        let codex = resolve(&settings, ToolId::Codex);

        let skill = item(CapabilityKind::Skill, "dev/repo-research");
        let claude_target = claude.target_path_for(&skill).unwrap();
        assert!(
            claude_target.ends_with("skills/repo-research"),
            "claude flattens to basename: {claude_target:?}"
        );
        let codex_target = codex.target_path_for(&skill).unwrap();
        assert!(
            codex_target.ends_with("skills/dev/repo-research"),
            "codex preserves nesting: {codex_target:?}"
        );
    }

    #[test]
    fn projection_modes_per_tool_kind() {
        let s = Settings::default();
        let cursor = resolve(&s, ToolId::Cursor);
        let codex = resolve(&s, ToolId::Codex);
        let openclaw = resolve(&s, ToolId::Openclaw);

        assert_eq!(
            cursor.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::FileSync)
        );
        assert_eq!(
            cursor.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            cursor.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            codex.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::MarkdownSectionSync)
        );
        assert_eq!(
            codex.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::JsonSection)
        );
        assert_eq!(openclaw.projection_mode_for(CapabilityKind::Hook), None);
        assert!(cursor.uses_managed_copy(CapabilityKind::Agent));
        assert!(!codex.uses_managed_copy(CapabilityKind::Agent));
    }

    #[test]
    fn hooks_have_no_per_item_target() {
        let s = Settings::default();
        let codex = resolve(&s, ToolId::Codex);
        let hook = item(CapabilityKind::Hook, "auto-format-after-edit");
        assert_eq!(codex.target_path_for(&hook), None);
    }

    #[test]
    fn rule_target_uses_rules_path_nested() {
        let s = Settings::default();
        let cursor = resolve(&s, ToolId::Cursor);
        let rule = item(CapabilityKind::Rule, "general/precise.mdc");
        let target = cursor.target_path_for(&rule).unwrap();
        assert!(target.ends_with(Path::new("rules/general/precise.mdc")));
    }
}
