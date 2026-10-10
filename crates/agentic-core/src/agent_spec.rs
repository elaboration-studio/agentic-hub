//! The optional `agent` block on a suite: the face (emoji), instructions, and
//! required CLIs that turn a suite into a runnable agent. See
//! `docs/tech/modules/agent-bundles.md`.

use serde::{Deserialize, Serialize};

pub const MAX_EMOJI_CHARS: usize = 16;
pub const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;
pub const MAX_REQUIRED_CLIS: usize = 20;
const MAX_CLI_ID_CHARS: usize = 64;

/// Agent identity layered on a suite. Persisted inside the suite entry in
/// `~/.agentic-suites.json`; older hubs ignore the field.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    #[serde(default)]
    pub emoji: Option<String>,
    /// Markdown prepended to the bundle's `instructions.md`.
    #[serde(default)]
    pub instructions: Option<String>,
    /// Ids from the CLI tool catalog (`resources/cli-tools/catalog.json`).
    #[serde(default)]
    pub required_clis: Vec<String>,
}

impl AgentSpec {
    /// Enforce the size limits. Catalog membership is not checked: an unknown
    /// id is reported as not installed when a bundle is built.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(emoji) = &self.emoji {
            if emoji.chars().count() > MAX_EMOJI_CHARS {
                return Err(format!(
                    "emoji must be at most {MAX_EMOJI_CHARS} characters"
                ));
            }
        }
        if let Some(instructions) = &self.instructions {
            if instructions.len() > MAX_INSTRUCTIONS_BYTES {
                return Err(format!(
                    "instructions must be at most {} KiB",
                    MAX_INSTRUCTIONS_BYTES / 1024
                ));
            }
        }
        if self.required_clis.len() > MAX_REQUIRED_CLIS {
            return Err(format!(
                "at most {MAX_REQUIRED_CLIS} required CLIs are allowed"
            ));
        }
        if let Some(bad) = self.required_clis.iter().find(|id| !is_cli_id(id)) {
            return Err(format!("required CLI id {bad:?} is not a catalog id"));
        }
        Ok(())
    }
}

fn is_cli_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= MAX_CLI_ID_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(emoji: &str, instructions: &str, clis: &[&str]) -> AgentSpec {
        AgentSpec {
            emoji: Some(emoji.to_string()),
            instructions: Some(instructions.to_string()),
            required_clis: clis.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn validate_accepts_values_at_every_limit() {
        let clis: Vec<String> = (0..MAX_REQUIRED_CLIS).map(|i| format!("cli-{i}")).collect();
        let s = AgentSpec {
            emoji: Some("é".repeat(MAX_EMOJI_CHARS)),
            instructions: Some("a".repeat(MAX_INSTRUCTIONS_BYTES)),
            required_clis: clis,
        };
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_emoji_over_sixteen_chars() {
        let s = spec(&"x".repeat(MAX_EMOJI_CHARS + 1), "", &[]);
        assert!(s.validate().unwrap_err().contains("emoji"));
    }

    #[test]
    fn validate_rejects_instructions_over_32_kib() {
        let s = spec("⚒️", &"a".repeat(MAX_INSTRUCTIONS_BYTES + 1), &[]);
        assert!(s.validate().unwrap_err().contains("instructions"));
    }

    #[test]
    fn validate_rejects_more_than_twenty_required_clis() {
        let clis: Vec<String> = (0..=MAX_REQUIRED_CLIS).map(|i| format!("c{i}")).collect();
        let refs: Vec<&str> = clis.iter().map(String::as_str).collect();
        let s = spec("⚒️", "", &refs);
        assert!(s.validate().unwrap_err().contains("required"));
    }

    #[test]
    fn validate_rejects_blank_or_spaced_cli_ids() {
        assert!(spec("⚒️", "", &[""]).validate().is_err());
        assert!(spec("⚒️", "", &["g h"]).validate().is_err());
    }

    #[test]
    fn missing_fields_deserialize_to_defaults() {
        let s: AgentSpec = serde_json::from_str("{}").unwrap();
        assert_eq!(s, AgentSpec::default());
    }

    #[test]
    fn serializes_required_clis_in_camel_case() {
        let json = serde_json::to_string(&spec("⚒️", "x", &["gh"])).unwrap();
        assert!(json.contains("\"requiredClis\":[\"gh\"]"), "{json}");
    }
}
