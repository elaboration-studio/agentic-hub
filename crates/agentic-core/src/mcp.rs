//! The `mcp` capability kind: one MCP server per `<root>/mcp/<id>/mcp.json`
//! (`agentic-hub.mcp.v1`). v1 never projects MCP into a tool home; bundles
//! carry it in `manifest.mcpServers`. See `docs/tech/modules/agent-bundles.md`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MCP_SCHEMA: &str = "agentic-hub.mcp.v1";
pub const MCP_MARKER: &str = "mcp.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
}

/// A validated server declaration. `env` holds variable names only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpManifest {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: Vec<String>,
}

/// Messages never echo field values: a mis-shaped `env` may hold a secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpManifestError {
    #[error("cannot read mcp.json: {0}")]
    Read(String),
    #[error("invalid mcp.json ({0})")]
    Parse(String),
    #[error("unsupported $schema (expected {MCP_SCHEMA})")]
    Schema,
    #[error("mcp server name is required")]
    MissingName,
    #[error("stdio transport requires a command")]
    MissingCommand,
    #[error("http transport requires an http(s) url")]
    MissingUrl,
    #[error("`{0}` is not used by the {1} transport")]
    UnexpectedField(&'static str, &'static str),
    #[error("env must be an array of variable names; values are never stored")]
    EnvNotNames,
}

/// `env` stays untyped so a mis-shaped value never reaches a serde message.
#[derive(Deserialize)]
struct RawManifest {
    #[serde(rename = "$schema", default)]
    schema: Option<String>,
    #[serde(default)]
    name: String,
    transport: McpTransport,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    env: Option<Value>,
}

/// Parse and validate an `mcp.json` document.
pub fn parse_manifest(json: &str) -> Result<McpManifest, McpManifestError> {
    let raw: RawManifest = serde_json::from_str(json).map_err(|e| {
        McpManifestError::Parse(format!(
            "{:?} error at line {}, column {}",
            e.classify(),
            e.line(),
            e.column()
        ))
    })?;
    if raw.schema.as_deref().is_some_and(|s| s != MCP_SCHEMA) {
        return Err(McpManifestError::Schema);
    }
    let name = raw.name.trim().to_string();
    if name.is_empty() {
        return Err(McpManifestError::MissingName);
    }
    let env = env_names(raw.env)?;
    let command = raw.command.filter(|c| !c.trim().is_empty());
    match raw.transport {
        McpTransport::Stdio => {
            if command.is_none() {
                return Err(McpManifestError::MissingCommand);
            }
            if raw.url.is_some() {
                return Err(McpManifestError::UnexpectedField("url", "stdio"));
            }
        }
        McpTransport::Http => {
            let is_http = raw
                .url
                .as_deref()
                .is_some_and(|u| u.starts_with("https://") || u.starts_with("http://"));
            if !is_http {
                return Err(McpManifestError::MissingUrl);
            }
            if command.is_some() || !raw.args.is_empty() {
                return Err(McpManifestError::UnexpectedField("command", "http"));
            }
        }
    }
    Ok(McpManifest {
        name,
        transport: raw.transport,
        command,
        args: raw.args,
        url: raw.url,
        env,
    })
}

/// Load and validate `<dir>/mcp.json`.
pub fn load_manifest(dir: &Path) -> Result<McpManifest, McpManifestError> {
    let text = std::fs::read_to_string(dir.join(MCP_MARKER))
        .map_err(|e| McpManifestError::Read(e.kind().to_string()))?;
    parse_manifest(&text)
}

fn env_names(value: Option<Value>) -> Result<Vec<String>, McpManifestError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(entries) = value else {
        return Err(McpManifestError::EnvNotNames);
    };
    entries
        .into_iter()
        .map(|entry| match entry {
            Value::String(name) if is_env_name(&name) => Ok(name),
            _ => Err(McpManifestError::EnvNotNames),
        })
        .collect()
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_registry;
    use crate::model::{CapabilityItem, CapabilityKind, SourceRef, ToolId};
    use crate::planner;
    use crate::settings::Settings;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn parses_stdio_server_with_env_names() {
        let m = parse_manifest(
            r#"{ "$schema": "agentic-hub.mcp.v1", "name": "github", "transport": "stdio",
                 "command": "github-mcp", "args": ["stdio"], "env": ["GITHUB_TOKEN"] }"#,
        )
        .unwrap();
        assert_eq!(
            m,
            McpManifest {
                name: "github".into(),
                transport: McpTransport::Stdio,
                command: Some("github-mcp".into()),
                args: vec!["stdio".into()],
                url: None,
                env: vec!["GITHUB_TOKEN".into()],
            }
        );
    }

    #[test]
    fn parses_http_server_without_schema_or_env() {
        let m = parse_manifest(
            r#"{ "name": "docs", "transport": "http", "url": "https://x.dev/mcp" }"#,
        )
        .unwrap();
        assert_eq!(m.transport, McpTransport::Http);
        assert_eq!(m.url.as_deref(), Some("https://x.dev/mcp"));
        assert!(m.env.is_empty());
    }

    #[test]
    fn rejects_env_object_with_values() {
        let err = parse_manifest(
            r#"{ "name": "g", "transport": "stdio", "command": "g", "env": { "GITHUB_TOKEN": "ghp_secret" } }"#,
        )
        .unwrap_err();
        assert_eq!(err, McpManifestError::EnvNotNames);
        assert!(!err.to_string().contains("ghp_secret"));
    }

    #[test]
    fn rejects_env_entry_carrying_a_value() {
        let err = parse_manifest(
            r#"{ "name": "g", "transport": "stdio", "command": "g", "env": ["GITHUB_TOKEN=ghp_secret"] }"#,
        )
        .unwrap_err();
        assert_eq!(err, McpManifestError::EnvNotNames);
    }

    #[test]
    fn parse_error_does_not_echo_string_values() {
        let err =
            parse_manifest(r#"{ "name": "g", "transport": "stdio", "command": ["ghp_secret"] }"#)
                .unwrap_err();
        assert!(matches!(err, McpManifestError::Parse(_)));
        assert!(!err.to_string().contains("ghp_secret"));
    }

    #[test]
    fn rejects_stdio_without_command() {
        let err = parse_manifest(r#"{ "name": "g", "transport": "stdio" }"#).unwrap_err();
        assert_eq!(err, McpManifestError::MissingCommand);
    }

    #[test]
    fn rejects_http_without_url() {
        let err = parse_manifest(r#"{ "name": "g", "transport": "http" }"#).unwrap_err();
        assert_eq!(err, McpManifestError::MissingUrl);
    }

    #[test]
    fn rejects_http_with_non_http_url() {
        let err = parse_manifest(r#"{ "name": "g", "transport": "http", "url": "file:///etc" }"#)
            .unwrap_err();
        assert_eq!(err, McpManifestError::MissingUrl);
    }

    #[test]
    fn rejects_unknown_schema() {
        let err = parse_manifest(
            r#"{ "$schema": "agentic-hub.mcp.v2", "name": "g", "transport": "http", "url": "https://x" }"#,
        )
        .unwrap_err();
        assert_eq!(err, McpManifestError::Schema);
    }

    #[test]
    fn rejects_missing_name() {
        let err = parse_manifest(r#"{ "transport": "http", "url": "https://x" }"#).unwrap_err();
        assert_eq!(err, McpManifestError::MissingName);
    }

    #[test]
    fn rejects_fields_of_the_other_transport() {
        let err = parse_manifest(
            r#"{ "name": "g", "transport": "http", "url": "https://x", "command": "g" }"#,
        )
        .unwrap_err();
        assert_eq!(err, McpManifestError::UnexpectedField("command", "http"));
    }

    #[test]
    fn planner_plans_nothing_for_mcp_on_any_tool() {
        let settings = Settings::default();
        let item = CapabilityItem {
            id: "mcp:github".into(),
            kind: CapabilityKind::Mcp,
            name: "github".into(),
            source_path: PathBuf::from("/src/mcp/github"),
            relative_path: PathBuf::from("github"),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            source: SourceRef {
                rel_home: "~/.agentic".into(),
                folder: ".agentic".into(),
            },
            valid: true,
            validation_errors: vec![],
        };
        let desired = HashMap::from([(item.id.clone(), true)]);
        for tool in ToolId::ALL {
            let adapter = adapter_registry::resolve(&settings, tool);
            assert_eq!(
                adapter.projection_mode_for(CapabilityKind::Mcp),
                None,
                "{tool:?}"
            );
            assert_eq!(adapter.target_path_for(&item), None, "{tool:?}");
            let items = std::slice::from_ref(&item);
            assert!(planner::build_plan(items, &adapter, &desired, false).is_empty());
            assert!(planner::inspect_tool(items, &adapter).is_empty());
        }
    }
}
