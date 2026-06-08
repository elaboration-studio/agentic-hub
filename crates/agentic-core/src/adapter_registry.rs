//! Per-tool adapter resolution. The single owner of the basename-vs-relative-path
//! decision (layout), the projection mode per `(tool, kind)`, and target-path
//! resolution. See `docs/tech/reference/tool-adapter-matrix.md`.

use std::path::PathBuf;

use crate::model::{CapabilityItem, CapabilityKind, ToolId};
use crate::settings::{Settings, ToolSettings};

/// How a kind's relative path maps onto the tool's target directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Collapse to basename (Claude's non-recursive *skill* loader).
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
    /// Slash-command directory. `None` when the tool has no command concept
    /// (OpenClaw).
    pub commands_path: Option<PathBuf>,
}

fn tool_settings(settings: &Settings, tool: ToolId) -> &ToolSettings {
    match tool {
        ToolId::Codex => &settings.tools.codex,
        ToolId::Claude => &settings.tools.claude,
        ToolId::Cursor => &settings.tools.cursor,
        ToolId::Openclaw => &settings.tools.openclaw,
        ToolId::Openstandard => &settings.tools.openstandard,
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
        // Fall back to the per-tool default so configs written before
        // `commands_path` existed still project commands (OpenClaw stays None).
        commands_path: ts
            .commands_path
            .clone()
            .or_else(|| crate::settings::default_commands_path(tool)),
    }
}

/// Resolve all tool adapters.
pub fn resolve_all(settings: &Settings) -> Vec<ResolvedAdapter> {
    ToolId::ALL.iter().map(|&t| resolve(settings, t)).collect()
}

/// Tools supported in workspace scope. OpenClaw and OpenStandard are global-only.
pub const WORKSPACE_TOOL_IDS: [ToolId; 3] = [ToolId::Codex, ToolId::Claude, ToolId::Cursor];

/// Materialize a workspace-scoped adapter rooted at `ws`. Paths are hard-coded
/// per tool (v1). OpenClaw and OpenStandard are unsupported and return a
/// disabled adapter so callers reject them. Notes: Codex subagents live in
/// `.codex/agents/*.toml`
/// (not `.agents/`, which holds only skills); Cursor reads `AGENTS.md` and the
/// shared `.agents/` dir in addition to its own `.cursor/` dirs. See
/// `docs/tech/modules/workspace-inventory.md`.
pub fn create_workspace_adapter(tool: ToolId, ws: &std::path::Path) -> ResolvedAdapter {
    let j = |p: &str| ws.join(p);
    match tool {
        ToolId::Codex => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".agents/skills"),
            agents_path: j(".codex/agents"),
            rules_path: j(".codex/agentic-rules"),
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: Some(j(".codex/hooks.json")),
            commands_path: Some(j(".codex/prompts")),
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
            commands_path: Some(j(".claude/commands")),
        },
        ToolId::Cursor => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".cursor/skills"),
            agents_path: j(".cursor/agents"),
            rules_path: j(".cursor/rules"),
            // Cursor reads a project-root `AGENTS.md` as agent instructions
            // (in addition to `.cursor/rules`), so the inventory attributes it
            // to Cursor too. Source: https://cursor.com/docs/rules (2026).
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: Some(j(".cursor/hooks.json")),
            commands_path: Some(j(".cursor/commands")),
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
            commands_path: None,
        },
        ToolId::Openstandard => ResolvedAdapter {
            tool_id: tool,
            enabled: false,
            skills_path: j(".agents/skills"),
            agents_path: j(".agents/agents"),
            rules_path: j(".agents/rules"),
            instructions_path: None,
            hooks_enabled: false,
            hooks_file: None,
            commands_path: None,
        },
    }
}

impl ResolvedAdapter {
    /// Layout for a kind. Only Claude *skills* are flat — Claude's skill loader
    /// scans `~/.claude/skills/` non-recursively, so nested source folders must
    /// collapse to their basename. Claude *agents* are scanned recursively (the
    /// loader walks `~/.claude/agents/` subfolders; identity comes from the
    /// `name` frontmatter), so they keep nesting like every other kind. See
    /// `docs/tech/reference/tool-adapter-matrix.md`.
    pub fn layout_for(&self, kind: CapabilityKind) -> Layout {
        match (self.tool_id, kind) {
            (ToolId::Claude, CapabilityKind::Skill) => Layout::Flat,
            _ => Layout::Nested,
        }
    }

    /// Projection mode for a `(tool, kind)` in global scope. `None` means the
    /// combination is unsupported (OpenClaw hooks).
    pub fn projection_mode_for(&self, kind: CapabilityKind) -> Option<ProjectionMode> {
        use CapabilityKind::{Agent, Command, Hook, Rule, Skill};
        use ProjectionMode::{FileSync, JsonSection, LinkSync, MarkdownSectionSync};
        match (self.tool_id, kind) {
            (ToolId::Openclaw, Hook | Command) => None,
            (_, Hook) => Some(JsonSection),
            (ToolId::Cursor, Agent) => Some(FileSync),
            // Claude's skill and command loaders do not follow symlinks, so they
            // are hard-copied (managed) rather than linked.
            (ToolId::Claude, Skill | Command) => Some(FileSync),
            (_, Command) => Some(LinkSync),
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
            CapabilityKind::Command => self.commands_path.as_ref(),
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
            CapabilityKind::Command => self.commands_path.as_ref()?,
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
            source: crate::model::SourceRef {
                rel_home: "~/.agentic".into(),
                folder: ".agentic".into(),
            },
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
        assert_eq!(codex.agents_path, ws.join(".codex/agents"));
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
        // Cursor reads a project-root AGENTS.md as instructions.
        assert_eq!(cursor.instructions_path, Some(ws.join("AGENTS.md")));
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
    fn workspace_adapter_openstandard_is_disabled() {
        let ws = Path::new("/ws");
        let os = create_workspace_adapter(ToolId::Openstandard, ws);
        assert!(!os.enabled, "OpenStandard is global-only");
        assert!(!os.hooks_enabled);
        assert_eq!(os.hooks_file, None);
        assert_eq!(os.instructions_path, None);
    }

    #[test]
    fn openstandard_global_adapter_owns_dot_agents() {
        let s = Settings::default();
        let os = resolve(&s, ToolId::Openstandard);
        assert!(os.enabled);
        assert!(os.skills_path.ends_with(".agents/skills"));
        assert!(os.agents_path.ends_with(".agents/agents"));
        assert!(os.rules_path.ends_with(".agents/rules"));
        // Open-standard skills/agents are symlinked; rules go to the AGENTS.md
        // managed block; hooks to the json section.
        assert_eq!(
            os.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            os.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::MarkdownSectionSync)
        );
        assert_eq!(
            os.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::JsonSection)
        );
    }

    #[test]
    fn claude_agent_is_nested_and_link_synced() {
        // Claude scans `~/.claude/agents/` recursively (subfolders allowed;
        // identity is the `name` frontmatter), so agents keep their nesting —
        // flattening would collide same-basename agents from different folders.
        // Source: https://code.claude.com/docs/en/sub-agents (2026).
        let s = Settings::default();
        let claude = resolve(&s, ToolId::Claude);
        assert_eq!(claude.layout_for(CapabilityKind::Agent), Layout::Nested);
        // Skills stay flat (the skill loader is non-recursive).
        assert_eq!(claude.layout_for(CapabilityKind::Skill), Layout::Flat);
        assert_eq!(
            claude.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::LinkSync)
        );

        let agent = item(CapabilityKind::Agent, "team/reviewer.md");
        let target = claude.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with("agents/team/reviewer.md"),
            "claude preserves agent nesting: {target:?}"
        );
    }

    #[test]
    fn command_targets_and_modes_per_tool() {
        let s = Settings::default();
        let cursor = resolve(&s, ToolId::Cursor);
        let codex = resolve(&s, ToolId::Codex);
        let claude = resolve(&s, ToolId::Claude);
        let os = resolve(&s, ToolId::Openstandard);
        let openclaw = resolve(&s, ToolId::Openclaw);

        // Commands are always nested.
        assert_eq!(cursor.layout_for(CapabilityKind::Command), Layout::Nested);

        // Cursor/Codex/OpenStandard symlink; Claude hard-copies; OpenClaw none.
        assert_eq!(
            cursor.projection_mode_for(CapabilityKind::Command),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            codex.projection_mode_for(CapabilityKind::Command),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            os.projection_mode_for(CapabilityKind::Command),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            claude.projection_mode_for(CapabilityKind::Command),
            Some(ProjectionMode::FileSync)
        );
        assert_eq!(openclaw.projection_mode_for(CapabilityKind::Command), None);
        assert!(claude.uses_managed_copy(CapabilityKind::Command));

        // Targets route to the per-tool commands dir, nested.
        let cmd = item(CapabilityKind::Command, "review/code-review.md");
        let cursor_target = cursor.target_path_for(&cmd).unwrap();
        assert!(
            cursor_target.ends_with(Path::new(".cursor/commands/review/code-review.md")),
            "{cursor_target:?}"
        );
        let codex_target = codex.target_path_for(&cmd).unwrap();
        assert!(
            codex_target.ends_with(Path::new(".codex/prompts/review/code-review.md")),
            "{codex_target:?}"
        );
        let claude_target = claude.target_path_for(&cmd).unwrap();
        assert!(
            claude_target.ends_with(Path::new(".claude/commands/review/code-review.md")),
            "{claude_target:?}"
        );

        // OpenClaw has no commands dir, so no target.
        assert_eq!(openclaw.target_path_for(&cmd), None);
        assert_eq!(openclaw.base_path_for(CapabilityKind::Command), None);
    }

    #[test]
    fn legacy_config_without_commands_path_falls_back_to_default() {
        // A config written before `commands_path` existed loads with `None`;
        // the adapter must still resolve the per-tool default so commands
        // project for existing users.
        let mut s = Settings::default();
        s.tools.cursor.commands_path = None;
        let cursor = resolve(&s, ToolId::Cursor);
        assert!(cursor
            .commands_path
            .as_ref()
            .unwrap()
            .ends_with(".cursor/commands"));
    }

    #[test]
    fn workspace_adapter_has_command_dirs() {
        let ws = Path::new("/ws");
        assert_eq!(
            create_workspace_adapter(ToolId::Codex, ws).commands_path,
            Some(ws.join(".codex/prompts"))
        );
        assert_eq!(
            create_workspace_adapter(ToolId::Claude, ws).commands_path,
            Some(ws.join(".claude/commands"))
        );
        assert_eq!(
            create_workspace_adapter(ToolId::Cursor, ws).commands_path,
            Some(ws.join(".cursor/commands"))
        );
        assert_eq!(
            create_workspace_adapter(ToolId::Openclaw, ws).commands_path,
            None
        );
    }
}
