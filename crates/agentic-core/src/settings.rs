use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{SourceRef, ToolId};
use crate::paths::{expand_tilde, home_dir, tildify};

/// A capability source: an ordered, priority-bearing shared root. See
/// `docs/tech/modules/multi-source-roots.md`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    /// Stable slug derived from `label`. Empty in the persisted file; filled by
    /// [`Settings::resolve_sources`].
    #[serde(default)]
    pub id: String,
    pub label: String,
    pub path: PathBuf,
}

impl SourceConfig {
    /// Portable, cross-device identity for this source. `rel_home` is the
    /// home-relative path (or the absolute path when outside `~`); `folder` is
    /// the last path component. Compute this from a resolved (tilde-expanded)
    /// source so the path is absolute.
    pub fn portable_ref(&self) -> SourceRef {
        let folder = self
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        SourceRef {
            rel_home: tildify(&self.path),
            folder,
        }
    }
}

/// True when `r` denotes a logical source present in `local`: matched by
/// home-relative path first, then by folder name. Used to decide whether a
/// synced suite reference applies on this machine.
pub fn source_present(local: &[SourceConfig], r: &SourceRef) -> bool {
    local.iter().any(|s| s.portable_ref().matches(r))
}

/// The editor used to open a capability's original file. `kind` is one of
/// `default` (OS default app), `vscode`, `cursor`, or `custom` (use
/// `custom_app`). Kept as a string so the JSON config stays forward-compatible
/// if more presets are added.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorPref {
    pub kind: String,
    /// Application name or path used when `kind == "custom"`.
    #[serde(default)]
    pub custom_app: Option<String>,
}

impl Default for EditorPref {
    fn default() -> Self {
        EditorPref {
            kind: "default".to_string(),
            custom_app: None,
        }
    }
}

impl EditorPref {
    /// The application name/path to hand to the opener, or `None` for the OS
    /// default app. macOS resolves these names via `open -a <name>`.
    pub fn app_name(&self) -> Option<String> {
        match self.kind.as_str() {
            "vscode" => Some("Visual Studio Code".to_string()),
            "cursor" => Some("Cursor".to_string()),
            "custom" => self
                .custom_app
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            _ => None,
        }
    }
}

/// Configuration for the skills.sh public skill source. Opt-in: off by default.
/// Search uses the keyless public index (no API key), so the only knob besides
/// the toggle is `favorites_path`, which optionally overrides where starred
/// skills are stored.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub favorites_path: Option<PathBuf>,
}

/// Anonymous usage telemetry (Aptabase). On by default; the user can disable it
/// in Config. The desktop shell tracks only coarse lifecycle events (app
/// start/exit) from Rust — the WebView never calls out.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        TelemetryConfig { enabled: true }
    }
}

/// Per-tool target paths and toggles. Mirrors the IPC `ToolSettings` shape.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSettings {
    pub enabled: bool,
    pub skills_path: PathBuf,
    pub agents_path: PathBuf,
    pub rules_path: PathBuf,
    pub instructions_path: Option<PathBuf>,
    pub hooks_enabled: bool,
    pub hooks_file: Option<PathBuf>,
    /// Per-hook JSON directory (Kiro/Copilot). `None` for tools that use a
    /// single `hooks_file` instead.
    #[serde(default)]
    pub hooks_dir: Option<PathBuf>,
    /// Directory holding slash-command prompts (`commands`/`prompts`). `None`
    /// when the tool has no command concept (OpenClaw). Injected for configs
    /// written before this field existed, so legacy files load without failing.
    #[serde(default)]
    pub commands_path: Option<PathBuf>,
}

#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsSettings {
    pub codex: ToolSettings,
    pub claude: ToolSettings,
    pub cursor: ToolSettings,
    pub openclaw: ToolSettings,
    /// Injected for configs written before this tool existed, so legacy files
    /// load instead of failing with a missing-field parse error.
    #[serde(default = "default_openstandard")]
    pub openstandard: ToolSettings,
    /// Injected for configs written before this tool existed.
    #[serde(default = "default_kiro")]
    pub kiro: ToolSettings,
    #[serde(default = "default_copilot")]
    pub copilot: ToolSettings,
    #[serde(default = "default_antigravity")]
    pub antigravity: ToolSettings,
}

fn default_openstandard() -> ToolSettings {
    ToolSettings::defaults_for(ToolId::Openstandard)
}

fn default_kiro() -> ToolSettings {
    ToolSettings::defaults_for(ToolId::Kiro)
}

fn default_copilot() -> ToolSettings {
    ToolSettings::defaults_for(ToolId::Copilot)
}

fn default_antigravity() -> ToolSettings {
    ToolSettings::defaults_for(ToolId::Antigravity)
}

/// Last-known main window size and position in logical pixels. Restored on
/// launch; updated when the user hides the main window.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainWindowState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
}

/// Global settings persisted at `~/.agentic-hub/config.json`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Ordered source forest. When empty, `shared_root` is the single Default.
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Deprecated single-root field; one-release fallback when `sources` empty.
    pub shared_root: PathBuf,
    /// Optional custom location for the suite store. `None` keeps the canonical
    /// `~/.agentic-suites.json` (migration-parity default).
    #[serde(default)]
    pub suites_path: Option<PathBuf>,
    /// Optional user-local CLI-tools catalog JSON, merged over the bundled
    /// catalog (override by id, append new). `None` uses only the bundled set.
    #[serde(default)]
    pub cli_tools_path: Option<PathBuf>,
    /// When on, the desktop shell watches the source roots and auto-reconciles
    /// projections on change. Defaults to on (the manual Rescan button is gone).
    #[serde(default = "default_true")]
    pub watcher_enabled: bool,
    /// One-time migration marker: 0.8.1 force-enables the watcher once (flipping
    /// configs that had paused it), then sets this so future user pauses stick.
    #[serde(default)]
    pub watcher_force_migrated: bool,
    /// One-time migration marker: rewrites a stale Codex `agentsPath` of
    /// `~/.agents/agents` (the pre-0.5.0 default, shared with OpenStandard) to
    /// the self-contained `~/.codex/agents` once, then sets this so a later
    /// deliberate choice of the old path sticks.
    #[serde(default)]
    pub codex_agents_path_migrated: bool,
    /// One-time migration marker: rewrites a stale Antigravity `skillsPath` of
    /// `~/.gemini/skills` (the pre-0.10.1 default) to `~/.gemini/config/skills`
    /// once, then sets this so a later deliberate choice of the old path sticks.
    #[serde(default)]
    pub antigravity_skills_path_migrated: bool,
    /// One-time migration marker: sets Kiro `instructionsPath` to
    /// `~/.kiro/steering/AGENTS.md` when absent so shared rules project into the
    /// AGENTS.md managed block instead of per-file steering copies.
    #[serde(default)]
    pub kiro_rules_agents_md_migrated: bool,
    /// Preferred editor for opening a capability's original file. Defaults to
    /// the OS default app.
    #[serde(default)]
    pub editor: EditorPref,
    /// Global accelerator that summons the command palette window. Stored as a
    /// human-readable accelerator string (e.g. `"Cmd+Alt+A"`). Defaults to
    /// `Cmd+Alt+A`.
    #[serde(default = "default_palette_shortcut")]
    pub palette_shortcut: String,
    /// When on, the palette pastes a command body into the focused app (macOS,
    /// needs Accessibility permission) instead of only copying. Defaults off.
    #[serde(default)]
    pub paste_into_focused: bool,
    /// Opt-in skills.sh public source config. Defaults to disabled.
    #[serde(default)]
    pub skills: SkillsConfig,
    /// Anonymous usage telemetry (Aptabase). Defaults to enabled.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Last main-window geometry; `None` uses `tauri.conf.json` defaults.
    #[serde(default)]
    pub main_window: Option<MainWindowState>,
    pub tools: ToolsSettings,
}

fn default_true() -> bool {
    true
}

/// Default global accelerator for the command palette.
pub fn default_palette_shortcut() -> String {
    "Cmd+Alt+A".to_string()
}

/// Lightweight sanity check for a palette accelerator string: it must be a
/// `+`-joined list of tokens carrying at least one non-modifier key. The
/// authoritative parse happens in the Tauri shell via the global-shortcut
/// plugin; this only rejects obviously-bad input before we persist or register.
pub fn is_valid_shortcut(accelerator: &str) -> bool {
    const MODIFIERS: [&str; 11] = [
        "cmd",
        "command",
        "ctrl",
        "control",
        "alt",
        "option",
        "shift",
        "super",
        "meta",
        "cmdorctrl",
        "commandorcontrol",
    ];
    let parts: Vec<&str> = accelerator
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return false;
    }
    parts
        .iter()
        .any(|p| !MODIFIERS.contains(&p.to_ascii_lowercase().as_str()))
}

/// Default slash-command directory per tool. `None` for OpenClaw (no command
/// concept). Codex uses `~/.codex/prompts`; the others use a `commands` dir.
/// Kept separate from [`ToolSettings::defaults_for`] so the adapter can fall
/// back to it when a legacy config has no `commands_path`.
pub fn default_commands_path(tool: ToolId) -> Option<PathBuf> {
    match tool {
        ToolId::Codex => Some(expand_tilde("~/.codex/prompts")),
        ToolId::Claude => Some(expand_tilde("~/.claude/commands")),
        ToolId::Cursor => Some(expand_tilde("~/.cursor/commands")),
        ToolId::Openstandard => Some(expand_tilde("~/.agents/commands")),
        ToolId::Openclaw | ToolId::Kiro | ToolId::Copilot | ToolId::Antigravity => None,
    }
}

/// Default per-hook JSON directory for tools that do not use a single hook
/// config file. Kept separate so adapter resolution can repair legacy configs
/// where `hooksDir` is absent.
pub fn default_hooks_dir(tool: ToolId) -> Option<PathBuf> {
    match tool {
        ToolId::Kiro => Some(expand_tilde("~/.kiro/hooks")),
        ToolId::Copilot => Some(expand_tilde("~/.copilot/hooks")),
        ToolId::Codex
        | ToolId::Claude
        | ToolId::Cursor
        | ToolId::Openclaw
        | ToolId::Openstandard
        | ToolId::Antigravity => None,
    }
}

/// Default instruction-file path per tool for markdown-section rule projection.
/// `None` when rules use symlinks (Cursor) or per-file steering only. Kept
/// separate so adapter resolution can repair legacy configs where
/// `instructionsPath` is absent (notably Kiro before AGENTS.md steering).
pub fn default_instructions_path(tool: ToolId) -> Option<PathBuf> {
    match tool {
        ToolId::Codex => Some(expand_tilde("~/.codex/AGENTS.md")),
        ToolId::Claude => Some(expand_tilde("~/.claude/CLAUDE.md")),
        ToolId::Openclaw => Some(expand_tilde("~/.openclaw/workspace/SOUL.md")),
        ToolId::Openstandard => Some(expand_tilde("~/.agents/AGENTS.md")),
        ToolId::Kiro => Some(expand_tilde("~/.kiro/steering/AGENTS.md")),
        ToolId::Copilot => Some(expand_tilde("~/.copilot/copilot-instructions.md")),
        ToolId::Antigravity => Some(expand_tilde("~/.gemini/AGENTS.md")),
        ToolId::Cursor => None,
    }
}

impl ToolSettings {
    fn defaults_for(tool: ToolId) -> ToolSettings {
        match tool {
            ToolId::Codex => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.codex/skills"),
                agents_path: expand_tilde("~/.codex/agents"),
                rules_path: expand_tilde("~/.codex/agentic-rules"),
                instructions_path: Some(expand_tilde("~/.codex/AGENTS.md")),
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.codex/hooks.json")),
                hooks_dir: None,
                commands_path: default_commands_path(ToolId::Codex),
            },
            ToolId::Claude => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.claude/skills"),
                agents_path: expand_tilde("~/.claude/agents"),
                rules_path: expand_tilde("~/.claude/rules"),
                instructions_path: Some(expand_tilde("~/.claude/CLAUDE.md")),
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.claude/settings.json")),
                hooks_dir: None,
                commands_path: default_commands_path(ToolId::Claude),
            },
            ToolId::Cursor => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.cursor/skills"),
                agents_path: expand_tilde("~/.cursor/agents"),
                rules_path: expand_tilde("~/.cursor/rules"),
                instructions_path: None,
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.cursor/hooks.json")),
                hooks_dir: None,
                commands_path: default_commands_path(ToolId::Cursor),
            },
            ToolId::Openclaw => ToolSettings {
                // Hidden by default; enable it in the Config page to project to it.
                enabled: false,
                skills_path: expand_tilde("~/.openclaw/skills"),
                agents_path: expand_tilde("~/.openclaw/agents"),
                rules_path: expand_tilde("~/.openclaw/agentic-rules"),
                instructions_path: Some(expand_tilde("~/.openclaw/workspace/SOUL.md")),
                hooks_enabled: false,
                hooks_file: None,
                hooks_dir: None,
                commands_path: default_commands_path(ToolId::Openclaw),
            },
            ToolId::Openstandard => ToolSettings {
                // The open-standard `~/.agents` root, shared across tools.
                enabled: true,
                skills_path: expand_tilde("~/.agents/skills"),
                agents_path: expand_tilde("~/.agents/agents"),
                rules_path: expand_tilde("~/.agents/rules"),
                instructions_path: Some(expand_tilde("~/.agents/AGENTS.md")),
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.agents/hooks.json")),
                hooks_dir: None,
                commands_path: default_commands_path(ToolId::Openstandard),
            },
            ToolId::Kiro => ToolSettings {
                enabled: false,
                skills_path: expand_tilde("~/.kiro/skills"),
                agents_path: expand_tilde("~/.kiro/agents"),
                rules_path: expand_tilde("~/.kiro/steering"),
                instructions_path: default_instructions_path(ToolId::Kiro),
                hooks_enabled: true,
                hooks_file: None,
                hooks_dir: default_hooks_dir(ToolId::Kiro),
                commands_path: None,
            },
            ToolId::Copilot => ToolSettings {
                enabled: false,
                skills_path: expand_tilde("~/.copilot/skills"),
                agents_path: expand_tilde("~/.copilot/agents"),
                rules_path: expand_tilde("~/.copilot/instructions"),
                instructions_path: Some(expand_tilde("~/.copilot/copilot-instructions.md")),
                hooks_enabled: true,
                hooks_file: None,
                hooks_dir: default_hooks_dir(ToolId::Copilot),
                commands_path: None,
            },
            ToolId::Antigravity => ToolSettings {
                enabled: false,
                skills_path: expand_tilde("~/.gemini/config/skills"),
                agents_path: expand_tilde("~/.gemini/antigravity/agents"),
                rules_path: expand_tilde("~/.gemini/antigravity/rules"),
                instructions_path: Some(expand_tilde("~/.gemini/AGENTS.md")),
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.gemini/config/hooks.json")),
                hooks_dir: None,
                commands_path: None,
            },
        }
    }
}

impl Default for ToolsSettings {
    fn default() -> Self {
        ToolsSettings {
            codex: ToolSettings::defaults_for(ToolId::Codex),
            claude: ToolSettings::defaults_for(ToolId::Claude),
            cursor: ToolSettings::defaults_for(ToolId::Cursor),
            openclaw: ToolSettings::defaults_for(ToolId::Openclaw),
            openstandard: ToolSettings::defaults_for(ToolId::Openstandard),
            kiro: ToolSettings::defaults_for(ToolId::Kiro),
            copilot: ToolSettings::defaults_for(ToolId::Copilot),
            antigravity: ToolSettings::defaults_for(ToolId::Antigravity),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sources: Vec::new(),
            shared_root: expand_tilde("~/.agentic"),
            suites_path: None,
            cli_tools_path: None,
            watcher_enabled: true,
            // A fresh config already has the watcher on; nothing to migrate.
            watcher_force_migrated: true,
            // A fresh config already ships the correct codex path; skip migration.
            codex_agents_path_migrated: true,
            // A fresh config already ships the correct antigravity path; skip migration.
            antigravity_skills_path_migrated: true,
            // A fresh config already ships Kiro AGENTS.md steering; skip migration.
            kiro_rules_agents_md_migrated: true,
            editor: EditorPref::default(),
            palette_shortcut: default_palette_shortcut(),
            paste_into_focused: false,
            skills: SkillsConfig::default(),
            telemetry: TelemetryConfig::default(),
            main_window: None,
            tools: ToolsSettings::default(),
        }
    }
}

impl Settings {
    /// Canonical config path: `~/.agentic-hub/config.json`.
    pub fn config_path() -> PathBuf {
        home_dir().join(".agentic-hub").join("config.json")
    }

    /// Effective suite-store path: the user's custom override (tilde-expanded)
    /// or the canonical `~/.agentic-suites.json`.
    pub fn resolved_suites_path(&self) -> PathBuf {
        match &self.suites_path {
            Some(p) => expand_tilde(&p.to_string_lossy()),
            None => crate::suite_store::default_path(),
        }
    }

    /// Effective skill-favorites path: the user's custom override (tilde-expanded)
    /// or the canonical `~/.agentic-hub/skills-favorites.json`.
    pub fn resolved_favorites_path(&self) -> PathBuf {
        match &self.skills.favorites_path {
            Some(p) => expand_tilde(&p.to_string_lossy()),
            None => crate::skill_favorites::default_path(),
        }
    }

    /// Effective user CLI-tools override path (tilde-expanded), or `None` when
    /// the user relies solely on the bundled catalog.
    pub fn resolved_cli_tools_path(&self) -> Option<PathBuf> {
        self.cli_tools_path
            .as_ref()
            .map(|p| expand_tilde(&p.to_string_lossy()))
    }

    /// One-time 0.8.1 migration: force the source watcher on, overriding a prior
    /// user pause exactly once, then mark it done so future pauses stick. Returns
    /// whether anything changed (and thus needs persisting). Idempotent.
    pub fn migrate_force_watcher_on(&mut self) -> bool {
        if self.watcher_force_migrated {
            return false;
        }
        self.watcher_enabled = true;
        self.watcher_force_migrated = true;
        true
    }

    /// One-time migration: rewrite a stale Codex `agents_path` of
    /// `~/.agents/agents` — the pre-0.5.0 default that collided with the
    /// OpenStandard-owned shared root, so Codex subagents never landed in
    /// `~/.codex/agents` where Codex reads them — to the current self-contained
    /// default. Only the exact superseded default is rewritten; any deliberate
    /// custom path is left alone. Returns whether the marker was newly set (and
    /// thus the config needs persisting). Idempotent.
    pub fn migrate_codex_agents_path(&mut self) -> bool {
        if self.codex_agents_path_migrated {
            return false;
        }
        self.codex_agents_path_migrated = true;
        if self.tools.codex.agents_path == expand_tilde("~/.agents/agents") {
            self.tools.codex.agents_path = expand_tilde("~/.codex/agents");
        }
        true
    }

    /// One-time migration: rewrite a stale Antigravity `skills_path` of
    /// `~/.gemini/skills` — the pre-0.10.1 default that Antigravity does not
    /// read per official docs — to `~/.gemini/config/skills`. Only the exact
    /// superseded default is rewritten; any deliberate custom path is left alone.
    /// Returns whether the marker was newly set (and thus the config needs
    /// persisting). Idempotent.
    pub fn migrate_antigravity_skills_path(&mut self) -> bool {
        if self.antigravity_skills_path_migrated {
            return false;
        }
        self.antigravity_skills_path_migrated = true;
        if self.tools.antigravity.skills_path == expand_tilde("~/.gemini/skills") {
            self.tools.antigravity.skills_path = expand_tilde("~/.gemini/config/skills");
        }
        true
    }

    /// One-time migration: set Kiro `instructions_path` to
    /// `~/.kiro/steering/AGENTS.md` when absent so shared rules use the AGENTS.md
    /// managed block per [Kiro steering docs](https://kiro.dev/docs/steering/).
    /// Returns whether the marker was newly set (and thus the config needs
    /// persisting). Idempotent.
    pub fn migrate_kiro_rules_agents_md(&mut self) -> bool {
        if self.kiro_rules_agents_md_migrated {
            return false;
        }
        self.kiro_rules_agents_md_migrated = true;
        if self.tools.kiro.instructions_path.is_none() {
            self.tools.kiro.instructions_path = default_instructions_path(ToolId::Kiro);
        }
        true
    }

    /// Load from the canonical path, falling back to defaults if absent.
    pub fn load() -> Result<Settings> {
        Self::load_from(&Self::config_path())
    }

    /// Load from an explicit path. Missing file → defaults; malformed JSON →
    /// `SettingsParse` (never silently overwrites the user's file).
    pub fn load_from(path: &Path) -> Result<Settings> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|e| CoreError::SettingsParse(e.to_string()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    /// Atomically write to the canonical path (`tmp` + `rename`).
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Resolve the effective, ordered source list with stable slug IDs and
    /// tilde-expanded paths. Empty `sources` → a single `Default` source from
    /// the legacy `shared_root`.
    pub fn resolve_sources(&self) -> Vec<SourceConfig> {
        let raw: Vec<SourceConfig> = if self.sources.is_empty() {
            vec![SourceConfig {
                id: String::new(),
                label: "Default".to_string(),
                path: self.shared_root.clone(),
            }]
        } else {
            self.sources.clone()
        };

        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut out = Vec::with_capacity(raw.len());
        for source in raw {
            let base = slugify(&source.label);
            let id = match counts.get_mut(&base) {
                Some(n) => {
                    *n += 1;
                    format!("{base}-{n}")
                }
                None => {
                    counts.insert(base.clone(), 1);
                    base
                }
            };
            let path = expand_tilde(&source.path.to_string_lossy());
            out.push(SourceConfig {
                id,
                label: source.label,
                path,
            });
        }
        out
    }
}

/// Lowercase, hyphen-separated slug. Non-alphanumerics collapse to a single
/// `-`; leading/trailing dashes are trimmed. Empty input → `"source"`.
pub fn slugify(label: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "source".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
impl Settings {
    /// Test-only: build settings whose every tool path is rooted under
    /// `tools_dir`, so a test can never read or write the real home directory.
    /// Pass a fresh tempdir per test. All four tools are enabled.
    pub(crate) fn sandboxed(shared_root: impl Into<PathBuf>, tools_dir: &Path) -> Self {
        let tool = |name: &str| {
            let base = tools_dir.join(name);
            ToolSettings {
                enabled: true,
                skills_path: base.join("skills"),
                agents_path: base.join("agents"),
                rules_path: base.join("rules"),
                instructions_path: Some(base.join("INSTRUCTIONS.md")),
                hooks_enabled: true,
                hooks_file: Some(base.join("hooks.json")),
                hooks_dir: None,
                commands_path: Some(base.join("commands")),
            }
        };
        let kiro_tool = || {
            let base = tools_dir.join("kiro");
            ToolSettings {
                enabled: true,
                skills_path: base.join("skills"),
                agents_path: base.join("agents"),
                rules_path: base.join("steering"),
                instructions_path: Some(base.join("steering/AGENTS.md")),
                hooks_enabled: true,
                hooks_file: None,
                hooks_dir: Some(base.join("hooks")),
                commands_path: None,
            }
        };
        let copilot_tool = || {
            let base = tools_dir.join("copilot");
            ToolSettings {
                enabled: true,
                skills_path: base.join("skills"),
                agents_path: base.join("agents"),
                rules_path: base.join("instructions"),
                instructions_path: Some(base.join("copilot-instructions.md")),
                hooks_enabled: true,
                hooks_file: None,
                hooks_dir: Some(base.join("hooks")),
                commands_path: None,
            }
        };
        let antigravity_tool = || {
            let base = tools_dir.join("antigravity");
            ToolSettings {
                enabled: true,
                skills_path: base.join("skills"),
                agents_path: base.join("agents"),
                rules_path: base.join("rules"),
                instructions_path: Some(base.join("AGENTS.md")),
                hooks_enabled: true,
                hooks_file: Some(base.join("hooks.json")),
                hooks_dir: None,
                commands_path: None,
            }
        };
        Settings {
            sources: Vec::new(),
            shared_root: shared_root.into(),
            suites_path: None,
            cli_tools_path: None,
            watcher_enabled: true,
            watcher_force_migrated: true,
            codex_agents_path_migrated: true,
            antigravity_skills_path_migrated: true,
            kiro_rules_agents_md_migrated: true,
            editor: EditorPref::default(),
            palette_shortcut: default_palette_shortcut(),
            paste_into_focused: false,
            skills: SkillsConfig::default(),
            telemetry: TelemetryConfig::default(),
            main_window: None,
            tools: ToolsSettings {
                codex: tool("codex"),
                claude: tool("claude"),
                cursor: tool("cursor"),
                openclaw: tool("openclaw"),
                openstandard: tool("openstandard"),
                kiro: kiro_tool(),
                copilot: copilot_tool(),
                antigravity: antigravity_tool(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_0_8_1_config_deserializes_unmigrated_and_force_on_flips_it_once() {
        // Simulate a pre-0.8.1 file: the watcher was paused and the migration
        // marker key does not exist yet (serde must fill it with false).
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("watcherForceMigrated");
        obj.insert("watcherEnabled".to_string(), serde_json::Value::Bool(false));
        let mut s: Settings = serde_json::from_value(value).unwrap();
        assert!(!s.watcher_enabled, "starts paused");
        assert!(!s.watcher_force_migrated, "marker absent in legacy file");

        // First migration forces it on and marks itself done.
        assert!(s.migrate_force_watcher_on());
        assert!(s.watcher_enabled);
        assert!(s.watcher_force_migrated);

        // Idempotent: a later user pause is respected, never re-forced.
        s.watcher_enabled = false;
        assert!(!s.migrate_force_watcher_on());
        assert!(!s.watcher_enabled);
    }

    #[test]
    fn pre_codex_agents_path_config_migrates_once_to_dot_codex() {
        // Simulate a config created before the codex `agentsPath` default moved
        // from `~/.agents/agents` (shared, OpenStandard-owned) to the
        // self-contained `~/.codex/agents`: the stale path is present and the
        // migration marker key does not exist (serde must fill it false).
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("codexAgentsPathMigrated");
        let codex = obj
            .get_mut("tools")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .get_mut("codex")
            .unwrap()
            .as_object_mut()
            .unwrap();
        codex.insert(
            "agentsPath".to_string(),
            serde_json::Value::String(
                expand_tilde("~/.agents/agents")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
        let mut s: Settings = serde_json::from_value(value).unwrap();
        assert!(
            !s.codex_agents_path_migrated,
            "marker absent in legacy file"
        );
        assert_eq!(s.tools.codex.agents_path, expand_tilde("~/.agents/agents"));

        // First migration moves the stale shared path into `.codex` and marks
        // itself done.
        assert!(s.migrate_codex_agents_path());
        assert_eq!(s.tools.codex.agents_path, expand_tilde("~/.codex/agents"));
        assert!(s.codex_agents_path_migrated);

        // Idempotent: a later deliberate choice of the old path is respected,
        // never re-moved.
        s.tools.codex.agents_path = expand_tilde("~/.agents/agents");
        assert!(!s.migrate_codex_agents_path());
        assert_eq!(s.tools.codex.agents_path, expand_tilde("~/.agents/agents"));
    }

    #[test]
    fn codex_agents_path_migration_leaves_custom_paths_untouched() {
        // A user who deliberately customized the codex agents path keeps it; the
        // migration only rewrites the exact superseded default.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("codexAgentsPathMigrated");
        obj.get_mut("tools")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .get_mut("codex")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(
                "agentsPath".to_string(),
                serde_json::Value::String("/custom/codex/agents".to_string()),
            );
        let mut s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.migrate_codex_agents_path(), "marker set on first run");
        assert_eq!(
            s.tools.codex.agents_path,
            PathBuf::from("/custom/codex/agents")
        );
        assert!(s.codex_agents_path_migrated);
    }

    #[test]
    fn pre_antigravity_skills_path_config_migrates_once_to_config_skills() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("antigravitySkillsPathMigrated");
        let antigravity = obj
            .get_mut("tools")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .get_mut("antigravity")
            .unwrap()
            .as_object_mut()
            .unwrap();
        antigravity.insert(
            "skillsPath".to_string(),
            serde_json::Value::String(
                expand_tilde("~/.gemini/skills")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
        let mut s: Settings = serde_json::from_value(value).unwrap();
        assert!(
            !s.antigravity_skills_path_migrated,
            "marker absent in legacy file"
        );
        assert_eq!(
            s.tools.antigravity.skills_path,
            expand_tilde("~/.gemini/skills")
        );

        assert!(s.migrate_antigravity_skills_path());
        assert_eq!(
            s.tools.antigravity.skills_path,
            expand_tilde("~/.gemini/config/skills")
        );
        assert!(s.antigravity_skills_path_migrated);

        assert!(!s.migrate_antigravity_skills_path());
    }

    #[test]
    fn antigravity_skills_path_migration_leaves_custom_path() {
        let mut s = Settings {
            antigravity_skills_path_migrated: false,
            ..Default::default()
        };
        s.tools.antigravity.skills_path = PathBuf::from("/custom/gemini/skills");
        assert!(s.migrate_antigravity_skills_path(), "marker set on first run");
        assert_eq!(
            s.tools.antigravity.skills_path,
            PathBuf::from("/custom/gemini/skills")
        );
        assert!(s.antigravity_skills_path_migrated);
    }

    #[test]
    fn kiro_rules_agents_md_migration_sets_instructions_path() {
        let mut s = Settings {
            kiro_rules_agents_md_migrated: false,
            tools: ToolsSettings {
                kiro: ToolSettings {
                    instructions_path: None,
                    ..ToolSettings::defaults_for(ToolId::Kiro)
                },
                ..ToolsSettings::default()
            },
            ..Default::default()
        };
        assert!(s.migrate_kiro_rules_agents_md());
        assert!(
            s.tools
                .kiro
                .instructions_path
                .as_ref()
                .unwrap()
                .ends_with(".kiro/steering/AGENTS.md")
        );
        assert!(s.kiro_rules_agents_md_migrated);
        assert!(!s.migrate_kiro_rules_agents_md());
    }

    #[test]
    fn defaults_match_tool_adapter_matrix() {
        let s = Settings::default();
        // Codex / Claude / Cursor / OpenStandard ship enabled; OpenClaw is hidden.
        assert!(s.tools.codex.enabled);
        assert!(s.tools.claude.enabled);
        assert!(s.tools.cursor.enabled);
        assert!(!s.tools.openclaw.enabled);
        assert!(s.tools.openstandard.enabled);
        assert!(!s.tools.kiro.enabled);
        assert!(s.tools.kiro.skills_path.ends_with(".kiro/skills"));
        assert!(s.tools.kiro.rules_path.ends_with(".kiro/steering"));
        assert!(s
            .tools
            .kiro
            .hooks_dir
            .as_ref()
            .unwrap()
            .ends_with(".kiro/hooks"));
        assert!(s.tools.kiro.commands_path.is_none());
        assert!(
            s.tools
                .kiro
                .instructions_path
                .as_ref()
                .unwrap()
                .ends_with(".kiro/steering/AGENTS.md")
        );
        assert!(!s.tools.copilot.enabled);
        assert!(s.tools.copilot.skills_path.ends_with(".copilot/skills"));
        assert!(s
            .tools
            .copilot
            .hooks_dir
            .as_ref()
            .unwrap()
            .ends_with(".copilot/hooks"));
        assert!(s
            .tools
            .copilot
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".copilot/copilot-instructions.md"));
        assert!(!s.tools.antigravity.enabled);
        assert!(s
            .tools
            .antigravity
            .skills_path
            .ends_with(".gemini/config/skills"));
        assert!(s
            .tools
            .antigravity
            .hooks_file
            .as_ref()
            .unwrap()
            .ends_with(".gemini/config/hooks.json"));
        assert!(s
            .tools
            .antigravity
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".gemini/AGENTS.md"));
        // Codex is self-contained under `.codex`; the open-standard `.agents`
        // root is owned by the OpenStandard tool.
        assert!(s.tools.codex.skills_path.ends_with(".codex/skills"));
        assert!(s.tools.openstandard.skills_path.ends_with(".agents/skills"));
        assert!(s.tools.openstandard.agents_path.ends_with(".agents/agents"));
        assert!(s.tools.openstandard.rules_path.ends_with(".agents/rules"));
        assert!(s
            .tools
            .openstandard
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".agents/AGENTS.md"));
        assert!(s
            .tools
            .openstandard
            .hooks_file
            .as_ref()
            .unwrap()
            .ends_with(".agents/hooks.json"));
        // Codex subagents are TOML files under `.codex/agents` (the `.agents/`
        // dir holds only skills). Source: developers.openai.com/codex/subagents.
        assert!(s.tools.codex.agents_path.ends_with(".codex/agents"));
        assert!(s.tools.codex.rules_path.ends_with(".codex/agentic-rules"));
        assert!(s
            .tools
            .codex
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".codex/AGENTS.md"));
        assert!(s.tools.claude.skills_path.ends_with(".claude/skills"));
        assert!(s.tools.cursor.instructions_path.is_none());
        // Slash-command dirs: Codex uses `prompts`, the rest a `commands` dir;
        // OpenClaw has none.
        assert!(s
            .tools
            .codex
            .commands_path
            .as_ref()
            .unwrap()
            .ends_with(".codex/prompts"));
        assert!(s
            .tools
            .claude
            .commands_path
            .as_ref()
            .unwrap()
            .ends_with(".claude/commands"));
        assert!(s
            .tools
            .cursor
            .commands_path
            .as_ref()
            .unwrap()
            .ends_with(".cursor/commands"));
        assert!(s
            .tools
            .openstandard
            .commands_path
            .as_ref()
            .unwrap()
            .ends_with(".agents/commands"));
        assert!(s.tools.openclaw.commands_path.is_none());
        // Watcher ships on; the manual Rescan button is replaced by the toggle.
        assert!(s.watcher_enabled);
        assert!(s
            .tools
            .openclaw
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".openclaw/workspace/SOUL.md"));
    }

    #[test]
    fn skills_source_defaults_off_and_roundtrips() {
        let s = Settings::default();
        assert!(!s.skills.enabled);
        assert!(s.skills.favorites_path.is_none());
        // Default favorites path is the canonical hub dotfile.
        assert!(s
            .resolved_favorites_path()
            .ends_with(".agentic-hub/skills-favorites.json"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let configured = Settings {
            skills: SkillsConfig {
                enabled: true,
                favorites_path: Some(PathBuf::from("/tmp/fav.json")),
            },
            ..Settings::default()
        };
        configured.save_to(&path).unwrap();
        let reloaded = Settings::load_from(&path).unwrap();
        assert_eq!(reloaded.skills, configured.skills);
        assert_eq!(
            reloaded.resolved_favorites_path(),
            PathBuf::from("/tmp/fav.json")
        );
    }

    #[test]
    fn telemetry_defaults_on_and_roundtrips() {
        let s = Settings::default();
        assert!(s.telemetry.enabled, "telemetry is on by default");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let configured = Settings {
            telemetry: TelemetryConfig { enabled: false },
            ..Settings::default()
        };
        configured.save_to(&path).unwrap();
        let reloaded = Settings::load_from(&path).unwrap();
        assert_eq!(reloaded.telemetry, configured.telemetry);
        assert!(!reloaded.telemetry.enabled);
    }

    #[test]
    fn legacy_config_without_telemetry_block_defaults_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        // A config written before the telemetry block existed (no `telemetry` key).
        fs::write(
            &path,
            r#"{ "sharedRoot": "/tmp/agentic", "tools": {
                "codex": {"enabled": true, "skillsPath": "/c/skills", "agentsPath": "/c/agents", "rulesPath": "/c/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "claude": {"enabled": true, "skillsPath": "/cl/skills", "agentsPath": "/cl/agents", "rulesPath": "/cl/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "cursor": {"enabled": true, "skillsPath": "/cu/skills", "agentsPath": "/cu/agents", "rulesPath": "/cu/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "openclaw": {"enabled": false, "skillsPath": "/o/skills", "agentsPath": "/o/agents", "rulesPath": "/o/rules", "instructionsPath": null, "hooksEnabled": false, "hooksFile": null}
            } }"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(loaded.telemetry.enabled, "absent block defaults to on");
    }

    #[test]
    fn legacy_config_without_skills_block_defaults_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        // A config written before the skills block existed (no `skills` key).
        fs::write(
            &path,
            r#"{ "sharedRoot": "/tmp/agentic", "tools": {
                "codex": {"enabled": true, "skillsPath": "/c/skills", "agentsPath": "/c/agents", "rulesPath": "/c/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "claude": {"enabled": true, "skillsPath": "/cl/skills", "agentsPath": "/cl/agents", "rulesPath": "/cl/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "cursor": {"enabled": true, "skillsPath": "/cu/skills", "agentsPath": "/cu/agents", "rulesPath": "/cu/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "openclaw": {"enabled": false, "skillsPath": "/o/skills", "agentsPath": "/o/agents", "rulesPath": "/o/rules", "instructionsPath": null, "hooksEnabled": false, "hooksFile": null}
            } }"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(!loaded.skills.enabled, "absent block defaults to off");
        // A config written before the openstandard tool existed has no
        // `openstandard` key; the serde default injects it instead of failing.
        assert!(loaded.tools.openstandard.enabled);
        assert!(loaded
            .tools
            .openstandard
            .skills_path
            .ends_with(".agents/skills"));
        assert!(!loaded.tools.kiro.enabled);
        assert!(loaded.tools.kiro.skills_path.ends_with(".kiro/skills"));
        assert!(!loaded.tools.copilot.enabled);
        assert!(loaded
            .tools
            .copilot
            .skills_path
            .ends_with(".copilot/skills"));
        assert!(!loaded.tools.antigravity.enabled);
        assert!(loaded
            .tools
            .antigravity
            .skills_path
            .ends_with(".gemini/config/skills"));
    }

    #[test]
    fn palette_shortcut_defaults_to_cmd_alt_a() {
        let s = Settings::default();
        assert_eq!(s.palette_shortcut, "Cmd+Alt+A");
    }

    #[test]
    fn paste_into_focused_defaults_off_and_roundtrips() {
        let s = Settings::default();
        assert!(!s.paste_into_focused);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let configured = Settings {
            paste_into_focused: true,
            ..Settings::default()
        };
        configured.save_to(&path).unwrap();
        let reloaded = Settings::load_from(&path).unwrap();
        assert!(reloaded.paste_into_focused);
    }

    #[test]
    fn legacy_config_without_paste_into_focused_defaults_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("pasteIntoFocused");
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(!loaded.paste_into_focused, "absent field defaults to off");
    }

    #[test]
    fn validates_palette_shortcut_shape() {
        assert!(is_valid_shortcut("Cmd+Alt+A"));
        assert!(is_valid_shortcut("CmdOrCtrl+Shift+K"));
        assert!(is_valid_shortcut("Space"));
        // Only modifiers, no key — rejected.
        assert!(!is_valid_shortcut("Cmd+Alt"));
        assert!(!is_valid_shortcut(""));
        assert!(!is_valid_shortcut("   "));
    }

    #[test]
    fn hook_defaults_per_tool() {
        let s = Settings::default();
        assert!(s.tools.codex.hooks_enabled);
        assert!(s.tools.claude.hooks_enabled);
        assert!(s.tools.cursor.hooks_enabled);
        assert!(!s.tools.openclaw.hooks_enabled);
        assert!(s
            .tools
            .cursor
            .hooks_file
            .as_ref()
            .unwrap()
            .ends_with(".cursor/hooks.json"));
        assert!(s
            .tools
            .claude
            .hooks_file
            .as_ref()
            .unwrap()
            .ends_with(".claude/settings.json"));
        assert!(s.tools.openclaw.hooks_file.is_none());
    }

    #[test]
    fn editor_pref_maps_to_app_name() {
        assert_eq!(EditorPref::default().app_name(), None);
        assert_eq!(
            EditorPref {
                kind: "vscode".into(),
                custom_app: None,
            }
            .app_name(),
            Some("Visual Studio Code".to_string())
        );
        assert_eq!(
            EditorPref {
                kind: "cursor".into(),
                custom_app: None,
            }
            .app_name(),
            Some("Cursor".to_string())
        );
        assert_eq!(
            EditorPref {
                kind: "custom".into(),
                custom_app: Some("  Zed  ".into()),
            }
            .app_name(),
            Some("Zed".to_string())
        );
        // Custom with empty string falls back to the OS default.
        assert_eq!(
            EditorPref {
                kind: "custom".into(),
                custom_app: Some("   ".into()),
            }
            .app_name(),
            None
        );
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify("Arno"), "arno");
        assert_eq!(slugify("Team Agentic"), "team-agentic");
        assert_eq!(slugify("  ~/Work//Stuff  "), "work-stuff");
        assert_eq!(slugify("***"), "source");
    }

    #[test]
    fn resolve_sources_legacy_fallback() {
        let s = Settings {
            shared_root: PathBuf::from("/tmp/agentic"),
            ..Settings::default()
        };
        let resolved = s.resolve_sources();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].label, "Default");
        assert_eq!(resolved[0].id, "default");
        assert_eq!(resolved[0].path, PathBuf::from("/tmp/agentic"));
    }

    #[test]
    fn resolve_sources_dedupes_slugs() {
        let s = Settings {
            sources: vec![
                SourceConfig {
                    id: String::new(),
                    label: "Arno".into(),
                    path: "/a".into(),
                },
                SourceConfig {
                    id: String::new(),
                    label: "Arno".into(),
                    path: "/b".into(),
                },
                SourceConfig {
                    id: String::new(),
                    label: "Arno".into(),
                    path: "/c".into(),
                },
            ],
            ..Settings::default()
        };
        let ids: Vec<String> = s.resolve_sources().into_iter().map(|x| x.id).collect();
        assert_eq!(ids, vec!["arno", "arno-2", "arno-3"]);
    }

    #[test]
    fn load_missing_returns_defaults_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded, Settings::default());

        loaded.save_to(&path).unwrap();
        let reloaded = Settings::load_from(&path).unwrap();
        assert_eq!(reloaded, loaded);
    }

    #[test]
    fn legacy_config_without_watcher_flag_defaults_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        // A config written before the watcher flag existed.
        fs::write(
            &path,
            r#"{ "sharedRoot": "/tmp/agentic", "tools": {
                "codex": {"enabled": true, "skillsPath": "/c/skills", "agentsPath": "/c/agents", "rulesPath": "/c/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "claude": {"enabled": true, "skillsPath": "/cl/skills", "agentsPath": "/cl/agents", "rulesPath": "/cl/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "cursor": {"enabled": true, "skillsPath": "/cu/skills", "agentsPath": "/cu/agents", "rulesPath": "/cu/rules", "instructionsPath": null, "hooksEnabled": true, "hooksFile": null},
                "openclaw": {"enabled": false, "skillsPath": "/o/skills", "agentsPath": "/o/agents", "rulesPath": "/o/rules", "instructionsPath": null, "hooksEnabled": false, "hooksFile": null}
            } }"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(loaded.watcher_enabled, "absent flag defaults to on");
    }

    #[test]
    fn load_malformed_is_error_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not valid json").unwrap();
        let result = Settings::load_from(&path);
        assert!(matches!(result, Err(CoreError::SettingsParse(_))));
        // File is untouched.
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not valid json");
    }
}
