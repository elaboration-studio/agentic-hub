//! Per-tool adapter resolution. The single owner of the basename-vs-relative-path
//! decision (layout), the projection mode per `(tool, kind)`, and target-path
//! resolution. See `docs/tech/reference/tool-adapter-matrix.md`.

use std::path::{Path, PathBuf};

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
    /// Managed copy whose bytes are rendered from the source (Codex agents:
    /// markdown → subagent TOML). Like `FileSync` for state/op purposes, but the
    /// target is renamed to `.toml` and written through a content transform.
    CodexAgentToml,
    MarkdownSectionSync,
    JsonSection,
    /// One Kiro v1 hook JSON file per hook id under `hooks_dir`.
    KiroHookFile,
    /// One Copilot v1 hook JSON file per hook id under `hooks_dir`.
    CopilotHookFile,
    /// One Claude-style JSON file per hook id under `~/.grok/hooks/`.
    GrokHookFile,
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
    /// Per-hook JSON directory (Kiro, Copilot, and Grok). `None` when hooks use
    /// `hooks_file`.
    pub hooks_dir: Option<PathBuf>,
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
        ToolId::Kiro => &settings.tools.kiro,
        ToolId::Copilot => &settings.tools.copilot,
        ToolId::Antigravity => &settings.tools.antigravity,
        ToolId::Grok => &settings.tools.grok,
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
        instructions_path: ts
            .instructions_path
            .clone()
            .or_else(|| crate::settings::default_instructions_path(tool)),
        hooks_enabled: ts.hooks_enabled,
        hooks_file: ts.hooks_file.clone(),
        // Fall back for configs persisted before per-file hook adapters added
        // `hooksDir`. `hooksEnabled` remains the explicit off switch.
        hooks_dir: ts
            .hooks_dir
            .clone()
            .or_else(|| crate::settings::default_hooks_dir(tool)),
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
pub const WORKSPACE_TOOL_IDS: [ToolId; 7] = [
    ToolId::Codex,
    ToolId::Claude,
    ToolId::Cursor,
    ToolId::Kiro,
    ToolId::Copilot,
    ToolId::Antigravity,
    ToolId::Grok,
];

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
            hooks_dir: None,
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
            hooks_dir: None,
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
            hooks_dir: None,
            commands_path: Some(j(".cursor/commands")),
        },
        ToolId::Kiro => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".kiro/skills"),
            agents_path: j(".kiro/agents"),
            rules_path: j(".kiro/steering"),
            // Kiro reads workspace-root AGENTS.md (always included); native
            // steering with inclusion modes lives under `.kiro/steering/*.md`.
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: None,
            hooks_dir: Some(j(".kiro/hooks")),
            commands_path: None,
        },
        ToolId::Copilot => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".github/skills"),
            agents_path: j(".github/agents"),
            rules_path: j(".github/instructions"),
            instructions_path: Some(j(".github/copilot-instructions.md")),
            hooks_enabled: true,
            hooks_file: None,
            hooks_dir: Some(j(".github/hooks")),
            commands_path: Some(j(".github/prompts")),
        },
        ToolId::Antigravity => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".agents/skills"),
            agents_path: j(".agents/agents"),
            rules_path: j(".agents/rules"),
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: Some(j(".agents/hooks.json")),
            hooks_dir: None,
            commands_path: None,
        },
        ToolId::Grok => ResolvedAdapter {
            tool_id: tool,
            enabled: true,
            skills_path: j(".grok/skills"),
            agents_path: j(".grok/agents"),
            rules_path: j(".grok/rules"),
            instructions_path: Some(j("AGENTS.md")),
            hooks_enabled: true,
            hooks_file: None,
            hooks_dir: Some(j(".grok/hooks")),
            commands_path: Some(j(".grok/commands")),
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
            hooks_dir: None,
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
            hooks_dir: None,
            commands_path: None,
        },
    }
}

impl ResolvedAdapter {
    /// Layout for a kind. Flat collapses a nested source path to its basename;
    /// Nested preserves it. Flattening is required wherever a tool's loader is
    /// **non-recursive** (it scans only the top level of the target dir):
    ///
    /// - Claude / Kiro / Copilot / Antigravity *skills* — top-level scan only;
    ///   Kiro and Antigravity also ignore symlinks under their tool dirs
    ///   ([kiro#6401](https://github.com/kirodotdev/Kiro/issues/6401),
    ///   [skills#633](https://github.com/vercel-labs/skills/issues/633)).
    /// - Cursor *agents* — `~/.cursor/agents/` is scanned non-recursively and
    ///   identity is the filename ([Cursor subagents](https://cursor.com/docs/subagents)).
    /// - Codex *agents* — only top-level `*.toml` are loaded from
    ///   `~/.codex/agents/` ([Codex subagents](https://developers.openai.com/codex/subagents)).
    ///
    /// Claude *agents* are the exception: that loader walks subfolders and takes
    /// identity from the `name` frontmatter, so they stay nested (flattening
    /// would collide same-basename agents). Same-basename collisions from
    /// flattening are resolved deterministically by the planner
    /// (`resolve_target_collisions`). See
    /// `docs/tech/reference/tool-adapter-matrix.md`.
    pub fn layout_for(&self, kind: CapabilityKind) -> Layout {
        use CapabilityKind::{Agent, Skill};
        match (self.tool_id, kind) {
            (ToolId::Claude | ToolId::Kiro | ToolId::Copilot | ToolId::Antigravity, Skill) => {
                Layout::Flat
            }
            (
                ToolId::Cursor | ToolId::Codex | ToolId::Kiro | ToolId::Copilot | ToolId::Grok,
                Agent,
            ) => Layout::Flat,
            (ToolId::Grok, CapabilityKind::Command) => Layout::Flat,
            _ => Layout::Nested,
        }
    }

    /// Projection mode for a `(tool, kind)` in global scope. `None` means the
    /// combination is unsupported (OpenClaw hooks).
    pub fn projection_mode_for(&self, kind: CapabilityKind) -> Option<ProjectionMode> {
        use CapabilityKind::{Agent, Command, Hook, Rule, Skill};
        use ProjectionMode::{
            CodexAgentToml, CopilotHookFile, FileSync, GrokHookFile, JsonSection, KiroHookFile,
            LinkSync, MarkdownSectionSync,
        };
        match (self.tool_id, kind) {
            (ToolId::Openclaw, Hook | Command) => None,
            (ToolId::Antigravity, Agent | Command) => None,
            (ToolId::Kiro, Hook) => Some(KiroHookFile),
            (ToolId::Kiro, Command) => None,
            (ToolId::Copilot, Hook) => Some(CopilotHookFile),
            (ToolId::Copilot, Command) => None,
            (ToolId::Grok, Hook) => Some(GrokHookFile),
            (ToolId::Grok, Rule) => Some(LinkSync),
            (_, Hook) => Some(JsonSection),
            (ToolId::Cursor, Agent) => Some(FileSync),
            (ToolId::Codex, Agent) => Some(CodexAgentToml),
            // Kiro's loader does not follow symlinks under ~/.kiro/ (skills,
            // agents); hard-copy like Claude skills. Shared rules use the
            // AGENTS.md managed block at `~/.kiro/steering/AGENTS.md`.
            (ToolId::Kiro, Skill | Agent) => Some(FileSync),
            // Antigravity IDE ignores symlinks under ~/.gemini/; hard-copy skills.
            (ToolId::Antigravity, Skill) => Some(FileSync),
            // Claude's agent loader does not follow symlinks (same as skills/commands).
            (ToolId::Claude, Skill | Command | Agent) => Some(FileSync),
            (_, Command) => Some(LinkSync),
            (_, Skill | Agent) => Some(LinkSync),
            (ToolId::Cursor | ToolId::Copilot, Rule) => Some(LinkSync),
            (ToolId::Antigravity, Rule) => Some(MarkdownSectionSync),
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

    /// Whether this `(tool, kind)` projects as a managed copy — a real file
    /// recorded in the per-root manifest (Cursor agents, Claude/Kiro/Antigravity
    /// skills, Kiro agents, Claude commands, Codex agents). Codex agents are a
    /// transformed copy but still managed.
    pub fn uses_managed_copy(&self, kind: CapabilityKind) -> bool {
        matches!(
            self.projection_mode_for(kind),
            Some(ProjectionMode::FileSync | ProjectionMode::CodexAgentToml)
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
        let mut rel = match self.layout_for(item.kind) {
            Layout::Nested => item.relative_path.clone(),
            Layout::Flat => PathBuf::from(item.relative_path.file_name()?),
        };
        // Codex subagents must be `.toml`; the source markdown is rendered to
        // TOML on write, so the projected filename swaps its extension too.
        if self.projection_mode_for(item.kind) == Some(ProjectionMode::CodexAgentToml) {
            rel.set_extension("toml");
        }
        if self.tool_id == ToolId::Copilot {
            rel = copilot_target_rel(item.kind, rel);
        }
        if self.tool_id == ToolId::Grok {
            rel = grok_target_rel(item.kind, rel);
        }
        Some(base.join(rel))
    }
}

fn grok_target_rel(kind: CapabilityKind, rel: PathBuf) -> PathBuf {
    if kind != CapabilityKind::Rule {
        return rel;
    }
    match rel.extension().and_then(|ext| ext.to_str()) {
        Some("mdc") => rel.with_extension("md"),
        _ => rel,
    }
}

fn copilot_target_rel(kind: CapabilityKind, rel: PathBuf) -> PathBuf {
    use CapabilityKind::{Agent, Rule};
    match kind {
        Agent => {
            let stem = rel
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            rel.with_file_name(format!("{stem}.agent.md"))
        }
        Rule => {
            let stem = rel
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let parent = rel.parent().unwrap_or(Path::new(""));
            parent.join(format!("{stem}.instructions.md"))
        }
        _ => rel,
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
        // Codex agents render to a managed TOML copy, not a markdown symlink.
        assert_eq!(
            codex.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::CodexAgentToml)
        );
        assert!(codex.uses_managed_copy(CapabilityKind::Agent));
    }

    #[test]
    fn codex_agent_target_is_toml_flat() {
        let s = Settings::default();
        let codex = resolve(&s, ToolId::Codex);
        // Codex loads only top-level `*.toml` subagents (nested files are
        // ignored), so the source basename is collapsed and the extension swaps
        // md -> toml. Source: https://developers.openai.com/codex/subagents (2026).
        let agent = item(CapabilityKind::Agent, "team/cto.md");
        let target = codex.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with(Path::new(".codex/agents/cto.toml")),
            "{target:?}"
        );
        assert_eq!(codex.layout_for(CapabilityKind::Agent), Layout::Flat);
    }

    #[test]
    fn cursor_agent_is_flat() {
        // Cursor's subagent loader scans only the top level of `~/.cursor/agents/`
        // and derives identity from the filename, so a nested source spec must
        // collapse to its basename or Cursor never discovers it. Source:
        // https://cursor.com/docs/subagents (2026).
        let s = Settings::default();
        let cursor = resolve(&s, ToolId::Cursor);
        assert_eq!(cursor.layout_for(CapabilityKind::Agent), Layout::Flat);
        let agent = item(CapabilityKind::Agent, "zoom/cto.md");
        let target = cursor.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with(Path::new(".cursor/agents/cto.md")),
            "cursor flattens nested agents to basename: {target:?}"
        );
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
    fn claude_agent_is_nested_and_file_synced() {
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
            Some(ProjectionMode::FileSync)
        );

        let agent = item(CapabilityKind::Agent, "team/reviewer.md");
        let target = claude.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with("agents/team/reviewer.md"),
            "claude preserves agent nesting: {target:?}"
        );
    }

    #[test]
    fn kiro_projection_modes_and_layout() {
        let s = Settings::default();
        let kiro = resolve(&s, ToolId::Kiro);
        assert!(!kiro.enabled);
        assert_eq!(kiro.layout_for(CapabilityKind::Skill), Layout::Flat);
        assert_eq!(
            kiro.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::FileSync)
        );
        assert!(kiro.uses_managed_copy(CapabilityKind::Skill));
        assert_eq!(
            kiro.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::FileSync)
        );
        assert!(kiro.uses_managed_copy(CapabilityKind::Agent));
        assert_eq!(
            kiro.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::MarkdownSectionSync)
        );
        assert!(!kiro.uses_managed_copy(CapabilityKind::Rule));
        assert!(kiro
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".kiro/steering/AGENTS.md"));
        assert_eq!(
            kiro.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::KiroHookFile)
        );
        assert_eq!(kiro.projection_mode_for(CapabilityKind::Command), None);
        assert_eq!(kiro.layout_for(CapabilityKind::Agent), Layout::Flat);
        assert!(kiro.skills_path.ends_with(".kiro/skills"));
        assert!(kiro.rules_path.ends_with(".kiro/steering"));
        assert!(kiro.hooks_dir.as_ref().unwrap().ends_with(".kiro/hooks"));
        assert!(kiro.hooks_file.is_none());

        let skill = item(CapabilityKind::Skill, "dev/repo-research");
        let skill_target = kiro.target_path_for(&skill).unwrap();
        assert!(
            skill_target.ends_with(Path::new(".kiro/skills/repo-research")),
            "kiro flattens skills to basename: {skill_target:?}"
        );

        let agent = item(CapabilityKind::Agent, "team/reviewer.md");
        let target = kiro.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with(Path::new(".kiro/agents/reviewer.md")),
            "{target:?}"
        );
    }

    #[test]
    fn resolve_restores_default_instructions_path_for_legacy_kiro() {
        let mut settings = Settings::default();
        settings.tools.kiro.instructions_path = None;

        let kiro = resolve(&settings, ToolId::Kiro);
        assert!(kiro
            .instructions_path
            .as_ref()
            .is_some_and(|path| path.ends_with(".kiro/steering/AGENTS.md")));
    }

    #[test]
    fn resolve_restores_default_hook_dirs_for_legacy_settings() {
        let mut settings = Settings::default();
        settings.tools.kiro.hooks_dir = None;
        settings.tools.copilot.hooks_dir = None;

        let kiro = resolve(&settings, ToolId::Kiro);
        let copilot = resolve(&settings, ToolId::Copilot);

        assert!(kiro
            .hooks_dir
            .as_ref()
            .is_some_and(|path| path.ends_with(".kiro/hooks")));
        assert!(copilot
            .hooks_dir
            .as_ref()
            .is_some_and(|path| path.ends_with(".copilot/hooks")));
    }

    #[test]
    fn workspace_adapter_kiro_paths() {
        let ws = Path::new("/ws");
        let kiro = create_workspace_adapter(ToolId::Kiro, ws);
        assert!(kiro.enabled);
        assert_eq!(kiro.skills_path, ws.join(".kiro/skills"));
        assert_eq!(kiro.agents_path, ws.join(".kiro/agents"));
        assert_eq!(kiro.rules_path, ws.join(".kiro/steering"));
        assert_eq!(kiro.instructions_path, Some(ws.join("AGENTS.md")));
        assert_eq!(kiro.hooks_dir, Some(ws.join(".kiro/hooks")));
        assert!(kiro.commands_path.is_none());
    }

    #[test]
    fn copilot_agent_target_uses_agent_md_suffix() {
        let s = Settings::default();
        let copilot = resolve(&s, ToolId::Copilot);
        assert!(!copilot.enabled);
        assert_eq!(copilot.layout_for(CapabilityKind::Skill), Layout::Flat);
        assert_eq!(
            copilot.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            copilot.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            copilot.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::CopilotHookFile)
        );
        assert_eq!(copilot.projection_mode_for(CapabilityKind::Command), None);
        assert_eq!(copilot.layout_for(CapabilityKind::Agent), Layout::Flat);

        let agent = item(CapabilityKind::Agent, "security-auditor.md");
        let target = copilot.target_path_for(&agent).unwrap();
        assert!(
            target.ends_with(Path::new(".copilot/agents/security-auditor.agent.md")),
            "{target:?}"
        );

        let skill = item(CapabilityKind::Skill, "dev/repo-research");
        let skill_target = copilot.target_path_for(&skill).unwrap();
        assert!(
            skill_target.ends_with(Path::new(".copilot/skills/repo-research")),
            "copilot flattens skills to basename: {skill_target:?}"
        );

        let rule = item(CapabilityKind::Rule, "team/style.md");
        let rule_target = copilot.target_path_for(&rule).unwrap();
        assert!(
            rule_target.ends_with(Path::new("instructions/team/style.instructions.md")),
            "{rule_target:?}"
        );
    }

    #[test]
    fn antigravity_projection_modes() {
        let s = Settings::default();
        let ag = resolve(&s, ToolId::Antigravity);
        assert!(!ag.enabled);
        assert_eq!(ag.layout_for(CapabilityKind::Skill), Layout::Flat);
        assert_eq!(
            ag.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::FileSync)
        );
        assert!(ag.uses_managed_copy(CapabilityKind::Skill));
        assert_eq!(ag.projection_mode_for(CapabilityKind::Agent), None);
        assert_eq!(
            ag.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::MarkdownSectionSync)
        );
        assert_eq!(
            ag.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::JsonSection)
        );
        assert!(ag.skills_path.ends_with(".gemini/config/skills"));

        let skill = item(CapabilityKind::Skill, "dev/repo-research");
        let skill_target = ag.target_path_for(&skill).unwrap();
        assert!(
            skill_target.ends_with(Path::new(".gemini/config/skills/repo-research")),
            "antigravity flattens skills to basename: {skill_target:?}"
        );
        assert!(ag
            .hooks_file
            .as_ref()
            .unwrap()
            .ends_with(".gemini/config/hooks.json"));
    }

    #[test]
    fn workspace_adapter_copilot_and_antigravity_paths() {
        let ws = Path::new("/ws");
        let copilot = create_workspace_adapter(ToolId::Copilot, ws);
        assert_eq!(copilot.skills_path, ws.join(".github/skills"));
        assert_eq!(copilot.agents_path, ws.join(".github/agents"));
        assert_eq!(copilot.hooks_dir, Some(ws.join(".github/hooks")));

        let ag = create_workspace_adapter(ToolId::Antigravity, ws);
        assert_eq!(ag.skills_path, ws.join(".agents/skills"));
        assert_eq!(ag.rules_path, ws.join(".agents/rules"));
        assert_eq!(ag.hooks_file, Some(ws.join(".agents/hooks.json")));
    }

    #[test]
    fn workspace_tool_ids_includes_kiro_copilot_antigravity() {
        assert!(WORKSPACE_TOOL_IDS.contains(&ToolId::Kiro));
        assert!(WORKSPACE_TOOL_IDS.contains(&ToolId::Copilot));
        assert!(WORKSPACE_TOOL_IDS.contains(&ToolId::Antigravity));
        assert!(WORKSPACE_TOOL_IDS.contains(&ToolId::Grok));
    }

    #[test]
    fn grok_projection_modes_and_layout() {
        let s = Settings::default();
        let grok = resolve(&s, ToolId::Grok);
        assert!(!grok.enabled);
        assert_eq!(grok.layout_for(CapabilityKind::Skill), Layout::Nested);
        assert_eq!(
            grok.projection_mode_for(CapabilityKind::Skill),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(grok.layout_for(CapabilityKind::Agent), Layout::Flat);
        assert_eq!(
            grok.projection_mode_for(CapabilityKind::Agent),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            grok.projection_mode_for(CapabilityKind::Rule),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(grok.layout_for(CapabilityKind::Command), Layout::Flat);
        assert_eq!(
            grok.projection_mode_for(CapabilityKind::Command),
            Some(ProjectionMode::LinkSync)
        );
        assert_eq!(
            grok.projection_mode_for(CapabilityKind::Hook),
            Some(ProjectionMode::GrokHookFile)
        );
        assert!(grok.skills_path.ends_with(".grok/skills"));
        assert!(grok.agents_path.ends_with(".grok/agents"));
        assert!(grok.rules_path.ends_with(".grok/rules"));
        assert!(grok
            .commands_path
            .as_ref()
            .is_some_and(|path| path.ends_with(".grok/commands")));
        assert!(grok
            .hooks_dir
            .as_ref()
            .is_some_and(|path| path.ends_with(".grok/hooks")));
        assert!(grok.hooks_file.is_none());
        assert!(grok.instructions_path.is_none());

        let skill = item(CapabilityKind::Skill, "dev/repo-research");
        let skill_target = grok.target_path_for(&skill).unwrap();
        assert!(
            skill_target.ends_with(Path::new(".grok/skills/dev/repo-research")),
            "grok keeps nested skills: {skill_target:?}"
        );

        let agent = item(CapabilityKind::Agent, "team/reviewer.md");
        let agent_target = grok.target_path_for(&agent).unwrap();
        assert!(
            agent_target.ends_with(Path::new(".grok/agents/reviewer.md")),
            "grok flattens agents: {agent_target:?}"
        );

        let command = item(CapabilityKind::Command, "review/code-review.md");
        let command_target = grok.target_path_for(&command).unwrap();
        assert!(
            command_target.ends_with(Path::new(".grok/commands/code-review.md")),
            "grok flattens commands: {command_target:?}"
        );

        let mdc = item(CapabilityKind::Rule, "team/style.mdc");
        let mdc_target = grok.target_path_for(&mdc).unwrap();
        assert!(
            mdc_target.ends_with(Path::new(".grok/rules/team/style.md")),
            "grok rewrites .mdc rules to .md: {mdc_target:?}"
        );
        let md = item(CapabilityKind::Rule, "always.md");
        let md_target = grok.target_path_for(&md).unwrap();
        assert!(
            md_target.ends_with(Path::new(".grok/rules/always.md")),
            "{md_target:?}"
        );
    }

    #[test]
    fn workspace_adapter_grok_paths() {
        let ws = Path::new("/ws");
        let grok = create_workspace_adapter(ToolId::Grok, ws);
        assert!(grok.enabled);
        assert_eq!(grok.skills_path, ws.join(".grok/skills"));
        assert_eq!(grok.agents_path, ws.join(".grok/agents"));
        assert_eq!(grok.rules_path, ws.join(".grok/rules"));
        assert_eq!(grok.instructions_path, Some(ws.join("AGENTS.md")));
        assert_eq!(grok.hooks_dir, Some(ws.join(".grok/hooks")));
        assert_eq!(grok.commands_path, Some(ws.join(".grok/commands")));
    }

    #[test]
    fn resolve_restores_default_grok_hook_dir_for_legacy_settings() {
        let mut settings = Settings::default();
        settings.tools.grok.hooks_dir = None;
        let grok = resolve(&settings, ToolId::Grok);
        assert!(grok
            .hooks_dir
            .as_ref()
            .is_some_and(|path| path.ends_with(".grok/hooks")));
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
