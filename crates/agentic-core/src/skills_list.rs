//! `ehub skills list` — project, hub, and installed-tool skills for one harness.
//! Read-only. See `docs/tech/reference/ehub-cli-contract.md`.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::bundle::Harness;
use crate::model::{CapabilityItem, CapabilityKind, ToolId};
use crate::scanner;
use crate::settings::Settings;
use crate::workspace_inventory::{scan_installed_tools, scan_workspace};

pub const SKILL_LIST_CAP: usize = 2000;
const DESCRIPTION_READ: usize = 16 * 1024;
const DESCRIPTION_MAX: usize = 280;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillRow {
    name: String,
    description: Option<String>,
    path: String,
    scope: &'static str,
    source_label: String,
    workspace: Option<String>,
}

pub fn document(settings: &Settings, harness: Harness, workspaces: &[PathBuf]) -> Value {
    json!({ "schema": 1, "skills": rows(settings, harness, workspaces) })
}

fn rows(settings: &Settings, harness: Harness, workspaces: &[PathBuf]) -> Vec<SkillRow> {
    let tool = harness.tool();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();

    for workspace in workspaces {
        let label = project_label(workspace);
        for item in scan_workspace(workspace, &[tool]).items {
            push_skill(
                &mut out,
                &mut seen,
                &item,
                "project",
                label.clone(),
                Some(workspace.display().to_string()),
            );
        }
    }

    for item in scanner::scan_all(&settings.resolve_sources()).items {
        push_skill(
            &mut out,
            &mut seen,
            &item,
            "hub",
            item.source_label.clone(),
            None,
        );
    }

    let installed = scan_installed_tools(settings);
    let own = format!("installed:{}", tool.as_str());
    let openstandard = format!("installed:{}", ToolId::Openstandard.as_str());
    let include_openstandard = matches!(harness, Harness::Cursor | Harness::Codex | Harness::Grok);
    for item in installed.items {
        let label = if item.source_id == openstandard {
            if !include_openstandard {
                continue;
            }
            ".agents".to_string()
        } else if item.source_id == own {
            tool_label(tool).to_string()
        } else {
            continue;
        };
        push_skill(&mut out, &mut seen, &item, "tool", label, None);
    }

    out
}

fn push_skill(
    out: &mut Vec<SkillRow>,
    seen: &mut HashSet<PathBuf>,
    item: &CapabilityItem,
    scope: &'static str,
    source_label: String,
    workspace: Option<String>,
) {
    if out.len() >= SKILL_LIST_CAP || item.kind != CapabilityKind::Skill {
        return;
    }
    let file = skill_md(&item.source_path);
    let Some(canonical) = file.canonicalize().ok() else {
        return;
    };
    if !seen.insert(canonical.clone()) {
        return;
    }
    let Some(name) = clean_name(&item.name) else {
        return;
    };
    out.push(SkillRow {
        name,
        description: skill_description(&canonical),
        path: canonical.display().to_string(),
        scope,
        source_label,
        workspace,
    });
}

fn skill_md(source_path: &Path) -> PathBuf {
    if source_path.ends_with("SKILL.md") {
        source_path.to_path_buf()
    } else {
        source_path.join("SKILL.md")
    }
}

fn project_label(workspace: &Path) -> String {
    let repo = workspace
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "project".to_string());
    format!("Project · {repo}")
}

fn tool_label(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Cursor => "Cursor",
        ToolId::Claude => "Claude",
        ToolId::Codex => "Codex",
        ToolId::Grok => "Grok",
        ToolId::Openstandard => ".agents",
        ToolId::Openclaw => "OpenClaw",
        ToolId::Kiro => "Kiro",
        ToolId::Copilot => "Copilot",
        ToolId::Antigravity => "Antigravity",
    }
}

fn clean_name(name: &str) -> Option<String> {
    let collapsed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

fn skill_description(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buf = [0_u8; DESCRIPTION_READ];
    let n = file.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let end = rest.find("\n---").unwrap_or(rest.len());
    for line in rest[..end].lines() {
        let Some(value) = line.strip_prefix("description:") else {
            continue;
        };
        let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
        let unquoted = collapsed
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                collapsed
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(collapsed.as_str());
        let trimmed = unquoted.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.chars().take(DESCRIPTION_MAX).collect());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, SourceConfig, ToolSettings};
    use std::fs;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn disabled(path: &Path) -> ToolSettings {
        ToolSettings {
            enabled: false,
            skills_path: path.to_path_buf(),
            agents_path: path.to_path_buf(),
            rules_path: path.to_path_buf(),
            instructions_path: Some(path.to_path_buf()),
            hooks_enabled: false,
            hooks_file: None,
            hooks_dir: None,
            commands_path: None,
        }
    }

    fn settings(home: &Path) -> Settings {
        let off = disabled(&home.join("off"));
        let mut settings = Settings {
            sources: vec![SourceConfig {
                id: "hub".into(),
                label: "Arno".into(),
                path: home.join("hub"),
            }],
            tools: crate::settings::ToolsSettings {
                codex: off.clone(),
                claude: off.clone(),
                cursor: ToolSettings {
                    enabled: true,
                    skills_path: home.join("cursor/skills"),
                    ..off.clone()
                },
                openclaw: off.clone(),
                openstandard: ToolSettings {
                    enabled: true,
                    skills_path: home.join("agents/skills"),
                    ..off.clone()
                },
                kiro: off.clone(),
                copilot: off.clone(),
                antigravity: off.clone(),
                grok: off,
            },
            ..Settings::default()
        };
        settings.tools.claude = ToolSettings {
            enabled: true,
            skills_path: home.join("claude/skills"),
            ..disabled(&home.join("off"))
        };
        settings
    }

    fn fixture(home: &Path) {
        write(
            &home.join("repo/.cursor/skills/tdd/SKILL.md"),
            "---\ndescription: Red   green\n---\n# tdd\n",
        );
        write(&home.join("hub/skills/review/SKILL.md"), "# review\n");
        write(
            &home.join("cursor/skills/ship/SKILL.md"),
            "---\ndescription: \"Ship it\"\n---\n",
        );
        write(&home.join("agents/skills/shared/SKILL.md"), "# shared\n");
        write(
            &home.join("claude/skills/only-claude/SKILL.md"),
            "# claude\n",
        );
        // Same file the project scan will see, so hub must not duplicate it.
        write(
            &home.join("repo/.cursor/skills/tdd/SKILL.md"),
            "---\ndescription: Red   green\n---\n# tdd\n",
        );
    }

    #[test]
    fn lists_project_then_hub_then_tool_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path());
        let settings = settings(dir.path());
        let doc = document(&settings, Harness::Cursor, &[dir.path().join("repo")]);
        let skills = doc["skills"].as_array().unwrap();
        let scopes: Vec<&str> = skills
            .iter()
            .map(|row| row["scope"].as_str().unwrap())
            .collect();
        assert_eq!(scopes, vec!["project", "hub", "tool", "tool"]);
        assert_eq!(skills[0]["name"], "tdd");
        assert_eq!(skills[0]["description"], "Red green");
        assert_eq!(skills[0]["sourceLabel"], "Project · repo");
        assert!(skills[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("tdd/SKILL.md"));
        assert_eq!(skills[1]["name"], "review");
        assert_eq!(skills[1]["description"], Value::Null);
        assert_eq!(skills[1]["sourceLabel"], "Arno");
        assert_eq!(skills[2]["sourceLabel"], "Cursor");
        assert_eq!(skills[2]["description"], "Ship it");
        assert_eq!(skills[3]["sourceLabel"], ".agents");
        assert_eq!(skills[3]["name"], "shared");
    }

    #[test]
    fn claude_omits_openstandard_skills() {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path());
        let settings = settings(dir.path());
        let doc = document(&settings, Harness::Claude, &[]);
        let names: Vec<&str> = doc["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["review", "only-claude"]);
    }

    #[test]
    fn the_same_workspace_twice_emits_the_skill_once() {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path());
        let settings = settings(dir.path());
        let repo = dir.path().join("repo");
        let doc = document(&settings, Harness::Cursor, &[repo.clone(), repo]);
        let project: Vec<&str> = doc["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["scope"] == "project")
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert_eq!(project, vec!["tdd"]);
    }
}
