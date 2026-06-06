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
}

fn default_openstandard() -> ToolSettings {
    ToolSettings::defaults_for(ToolId::Openstandard)
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
    /// When on, the desktop shell watches the source roots and auto-reconciles
    /// projections on change. Defaults to on (the manual Rescan button is gone).
    #[serde(default = "default_true")]
    pub watcher_enabled: bool,
    /// Preferred editor for opening a capability's original file. Defaults to
    /// the OS default app.
    #[serde(default)]
    pub editor: EditorPref,
    /// Global accelerator that summons the command palette window. Stored as a
    /// human-readable accelerator string (e.g. `"Cmd+Alt+A"`). Defaults to
    /// `Cmd+Alt+A`.
    #[serde(default = "default_palette_shortcut")]
    pub palette_shortcut: String,
    /// Opt-in skills.sh public source config. Defaults to disabled.
    #[serde(default)]
    pub skills: SkillsConfig,
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
            },
            ToolId::Claude => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.claude/skills"),
                agents_path: expand_tilde("~/.claude/agents"),
                rules_path: expand_tilde("~/.claude/rules"),
                instructions_path: Some(expand_tilde("~/.claude/CLAUDE.md")),
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.claude/settings.json")),
            },
            ToolId::Cursor => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.cursor/skills"),
                agents_path: expand_tilde("~/.cursor/agents"),
                rules_path: expand_tilde("~/.cursor/rules"),
                instructions_path: None,
                hooks_enabled: true,
                hooks_file: Some(expand_tilde("~/.cursor/hooks.json")),
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
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sources: Vec::new(),
            shared_root: expand_tilde("~/.agentic"),
            suites_path: None,
            watcher_enabled: true,
            editor: EditorPref::default(),
            palette_shortcut: default_palette_shortcut(),
            skills: SkillsConfig::default(),
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
            }
        };
        Settings {
            sources: Vec::new(),
            shared_root: shared_root.into(),
            suites_path: None,
            watcher_enabled: true,
            editor: EditorPref::default(),
            palette_shortcut: default_palette_shortcut(),
            skills: SkillsConfig::default(),
            tools: ToolsSettings {
                codex: tool("codex"),
                claude: tool("claude"),
                cursor: tool("cursor"),
                openclaw: tool("openclaw"),
                openstandard: tool("openstandard"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_tool_adapter_matrix() {
        let s = Settings::default();
        // Codex / Claude / Cursor / OpenStandard ship enabled; OpenClaw is hidden.
        assert!(s.tools.codex.enabled);
        assert!(s.tools.claude.enabled);
        assert!(s.tools.cursor.enabled);
        assert!(!s.tools.openclaw.enabled);
        assert!(s.tools.openstandard.enabled);
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
    }

    #[test]
    fn palette_shortcut_defaults_to_cmd_alt_a() {
        let s = Settings::default();
        assert_eq!(s.palette_shortcut, "Cmd+Alt+A");
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
