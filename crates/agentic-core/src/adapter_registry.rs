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

/// Tools supported in workspace scope. OpenClaw is global-only.
pub const WORKSPACE_TOOL_IDS: [ToolId; 3] = [ToolId::Codex, ToolId::Claude, ToolId::Cursor];

/// Materialize a workspace-scoped adapter rooted at `ws`. Paths are hard-coded
/// per tool (v1). OpenClaw is unsupported and returns a disabled adapter so
/// callers reject it. See `docs/tech/modules/workspace-patch.md`.
pub fn create_workspace_adapter(tool: ToolId, ws: &std::path::Path) -> ResolvedAdapter {
    let j = |p: &str| ws.join(p);
    match tool {
        ToolId::Codex => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".agents/skills"),
            agents_path: j(".agents/agents"),
            rules_path: j(".codex/agentic-rules"),
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: Some(j(".codex/hooks.json")),
        },
        ToolId::Claude => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".claude/skills"),
            agents_path: j(".claude/agents"),
            rules_path: j(".claude/agentic-rules"),
            instructions_path: Some(j("CLAUDE.md")),
            hooks_enabled: true,
            hooks_file: Some(j(".claude/settings.json")),
        },
        ToolId::Cursor => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".cursor/skills"),
            agents_path: j(".cursor/agents"),
            rules_path: j(".cursor/rules"),
            instructions_path: None,
            hooks_enabled: true,
            hooks_file: Some(j(".cursor/hooks.json")),
        },
        ToolId::Openclaw => ResolvedAdapter {
            tool_id: tool,
            enabled: false,
            skills_path: j(".openclaw/skills"),
            agents_path: j(".openclaw/agents"),
            rules_path: j(".openclaw/agentic-rules"),
            instructions_path: None,
            hooks_enabled: false,
            hooks_file: None,
        },
    }
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
            // Claude's skill loader does not follow symlinks, so skills are
            // hard-copied (managed) rather than linked.
            (ToolId::Claude, Skill) => Some(FileSync),
            (_, Skill | Agent) => Some(LinkSync),
            (ToolId::Cursor, Rule) => Some(LinkSync),
            (_, Rule) => Some(MarkdownSectionSync),
        }
    }

    /// Base directory for an item's kind (`skills`/`agents`/`rules` dir). `None`
    /// for hooks, which target the single `hooks_file`.
    pub fn base_path_for(&self, kind: CapabilityKind) -> Option<&PathBuf> {
        match kind {
            CapabilityKind::Skill => Some(&self.skills_path),
            CapabilityKind::Agent => Some(&self.agents_path),
            CapabilityKind::Rule => Some(&self.rules_path),
            CapabilityKind::Hook => None,
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
        let claude = resolve(&s, ToolId::Claude);
        let openclaw = resolve(&s, ToolId::Openclaw);

        // Claude skills are hard-copied (loader does not follow symlinks).
        assert_eq!(
            claude.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::FileSync)
        );
        assert_eq!(
            codex.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::LinkSync)
        );

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

    #[test]
    fn workspace_adapter_paths_per_tool() {
        let ws = Path::new("/ws");

        let codex = create_workspace_adapter(ToolId::Codex, ws);
        assert_eq!(codex.skills_path, ws.join(".agents/skills"));
        assert_eq!(codex.agents_path, ws.join(".agents/agents"));
        assert_eq!(codex.rules_path, ws.join(".codex/agentic-rules"));
        assert_eq!(codex.instructions_path, Some(ws.join("AGENTS.md")));
        assert_eq!(codex.hooks_file, Some(ws.join(".codex/hooks.json")));
        assert!(codex.enabled && codex.hooks_enabled);

        let claude = create_workspace_adapter(ToolId::Claude, ws);
        assert_eq!(claude.skills_path, ws.join(".claude/skills"));
        assert_eq!(claude.agents_path, ws.join(".claude/agents"));
        assert_eq!(claude.rules_path, ws.join(".claude/agentic-rules"));
        assert_eq!(claude.instructions_path, Some(ws.join("CLAUDE.md")));
        assert_eq!(claude.hooks_file, Some(ws.join(".claude/settings.json")));

        let cursor = create_workspace_adapter(ToolId::Cursor, ws);
        assert_eq!(cursor.skills_path, ws.join(".cursor/skills"));
        assert_eq!(cursor.agents_path, ws.join(".cursor/agents"));
        assert_eq!(cursor.rules_path, ws.join(".cursor/rules"));
        assert_eq!(cursor.instructions_path, None);
        assert_eq!(cursor.hooks_file, Some(ws.join(".cursor/hooks.json")));
    }

    #[test]
    fn workspace_adapter_openclaw_is_disabled() {
        let ws = Path::new("/ws");
        let oc = create_workspace_adapter(ToolId::Openclaw, ws);
        assert!(!oc.enabled);
        assert!(!oc.hooks_enabled);
        assert_eq!(oc.hooks_file, None);
        assert_eq!(oc.instructions_path, None);
    }

    #[test]
    fn claude_agent_is_flat_and_link_synced() {
        let s = Settings::default();
        let claude = resolve(&s, ToolId::Claude);
        assert_eq!(claude.layout_for(CapabilityKind::Agent), Layout::Flat);
        assert_eq!(
            claude.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::LinkSync)
        );

        let agent = item(CapabilityKind::Agent, "team/reviewer.md");
        let target = claude.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with("agents/reviewer.md"),
            "claude flattens agents to basename: {target:?}"
        );
    }
}
