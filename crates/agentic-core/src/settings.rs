use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::ToolId;
use crate::paths::{expand_tilde, home_dir};

/// A capability source: an ordered, priority-bearing shared root. See
/// `docs/tech/modules/multi-source-roots.md`.
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

/// Per-tool target paths and toggles. Mirrors the IPC `ToolSettings` shape.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsSettings {
    pub codex: ToolSettings,
    pub claude: ToolSettings,
    pub cursor: ToolSettings,
    pub openclaw: ToolSettings,
}

/// Global settings persisted at `~/.agentic-hub/config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Ordered source forest. When empty, `shared_root` is the single Default.
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Deprecated single-root field; one-release fallback when `sources` empty.
    pub shared_root: PathBuf,
    pub tools: ToolsSettings,
}

impl ToolSettings {
    fn defaults_for(tool: ToolId) -> ToolSettings {
        match tool {
            ToolId::Codex => ToolSettings {
                enabled: true,
                skills_path: expand_tilde("~/.agents/skills"),
                agents_path: expand_tilde("~/.agents/agents"),
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
                enabled: true,
                skills_path: expand_tilde("~/.openclaw/skills"),
                agents_path: expand_tilde("~/.openclaw/agents"),
                rules_path: expand_tilde("~/.openclaw/agentic-rules"),
                instructions_path: Some(expand_tilde("~/.openclaw/workspace/SOUL.md")),
                hooks_enabled: false,
                hooks_file: None,
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
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sources: Vec::new(),
            shared_root: expand_tilde("~/.agentic"),
            tools: ToolsSettings::default(),
        }
    }
}

impl Settings {
    /// Canonical config path: `~/.agentic-hub/config.json`.
    pub fn config_path() -> PathBuf {
        home_dir().join(".agentic-hub").join("config.json")
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
mod tests {
    use super::*;

    #[test]
    fn defaults_match_tool_adapter_matrix() {
        let s = Settings::default();
        assert!(s.tools.codex.skills_path.ends_with(".agents/skills"));
        assert!(s.tools.codex.agents_path.ends_with(".agents/agents"));
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
        assert!(s
            .tools
            .openclaw
            .instructions_path
            .as_ref()
            .unwrap()
            .ends_with(".openclaw/workspace/SOUL.md"));
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
