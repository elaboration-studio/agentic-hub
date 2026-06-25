//! Markdown agent spec → Codex subagent TOML.
//!
//! Codex custom agents are standalone TOML files defining `name`,
//! `description`, and `developer_instructions`
//! (developers.openai.com/codex/subagents). Codex loads only `*.toml` from
//! `~/.codex/agents/` and parses them as TOML, so the markdown agent specs the
//! hub shares are silently ignored there. Every other tool (Cursor, Claude,
//! OpenStandard) reads the markdown as-is; Codex is the one tool that needs a
//! distinct serialization. This renders that TOML: the frontmatter `name` /
//! `description` map to the TOML fields and the markdown body becomes
//! `developer_instructions`.

use serde::Serialize;

#[derive(Serialize)]
struct CodexAgent {
    name: String,
    description: String,
    developer_instructions: String,
}

/// Render Codex subagent TOML from a markdown agent spec. `fallback_name` (the
/// file stem) fills `name` when the frontmatter omits it. A spec without
/// frontmatter yields the whole text as `developer_instructions` with an empty
/// description. Deterministic: the same input always renders identical bytes,
/// which keeps the projected copy's enabled/stale detection stable.
pub fn to_toml(markdown: &str, fallback_name: &str) -> String {
    let (front, body) = split_frontmatter(markdown);
    let name = front
        .as_deref()
        .and_then(|f| field(f, "name"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    let description = front
        .as_deref()
        .and_then(|f| field(f, "description"))
        .unwrap_or_default();
    let agent = CodexAgent {
        name,
        description,
        developer_instructions: body.trim().to_string(),
    };
    // A struct of three owned strings always serializes; the empty fallback
    // keeps the call infallible for the applier's hot path.
    toml::to_string(&agent).unwrap_or_default()
}

/// Split `---\n<frontmatter>\n---\n<body>`. Returns `(Some(frontmatter), body)`
/// when a leading frontmatter block is present, else `(None, whole input)`.
fn split_frontmatter(md: &str) -> (Option<String>, String) {
    let Some(rest) = md.strip_prefix("---\n") else {
        return (None, md.to_string());
    };
    if let Some((front, body)) = rest.split_once("\n---\n") {
        (Some(front.to_string()), body.to_string())
    } else if let Some((front, body)) = rest.split_once("\n---") {
        // Tolerate a closing fence with no trailing newline / empty body.
        (Some(front.to_string()), body.trim_start().to_string())
    } else {
        (None, md.to_string())
    }
}

/// Read a top-level scalar `key` from YAML-ish frontmatter, joining folded
/// continuation lines (indented, without their own top-level `key:`) with
/// single spaces. Surrounding quotes are stripped. This covers the plain and
/// folded scalars our agent specs use; it is not a full YAML parser.
fn field(front: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let mut lines = front.lines();
    let mut value = loop {
        let line = lines.next()?;
        // Only a top-level (unindented) key counts.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some(rest) = line.strip_prefix(&prefix) {
            break rest.trim().to_string();
        }
    };
    for line in lines {
        // A blank line or the next top-level key ends the folded scalar.
        if line.trim().is_empty() || !line.starts_with(char::is_whitespace) {
            break;
        }
        if !value.is_empty() {
            value.push(' ');
        }
        value.push_str(line.trim());
    }
    Some(strip_quotes(value))
}

fn strip_quotes(s: String) -> String {
    let b = s.as_bytes();
    let quoted = s.len() >= 2
        && ((b[0] == b'"' && b[s.len() - 1] == b'"') || (b[0] == b'\'' && b[s.len() - 1] == b'\''));
    if quoted {
        s[1..s.len() - 1].to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Parsed {
        name: String,
        description: String,
        developer_instructions: String,
    }

    fn parse(toml_str: &str) -> Parsed {
        toml::from_str(toml_str).expect("rendered TOML must be valid")
    }

    #[test]
    fn frontmatter_maps_to_name_description_body() {
        let md = "---\nname: cto\ndescription: Chief Technology Officer.\n---\n\n# CTO\n\nBuild clear systems.\n";
        let out = to_toml(md, "ignored-fallback");
        let p = parse(&out);
        assert_eq!(p.name, "cto");
        assert_eq!(p.description, "Chief Technology Officer.");
        assert_eq!(p.developer_instructions, "# CTO\n\nBuild clear systems.");
    }

    #[test]
    fn folded_description_lines_join_with_spaces() {
        // YAML plain folded scalar: continuation lines are indented.
        let md = "---\nname: cto\ndescription: Chief Technology Officer —\n  architecture, delivery,\n  and review.\n---\nBody.\n";
        let p = parse(&to_toml(md, "cto"));
        assert_eq!(
            p.description,
            "Chief Technology Officer — architecture, delivery, and review."
        );
    }

    #[test]
    fn missing_frontmatter_uses_fallback_name_and_whole_body() {
        let md = "# Just a heading\n\nSome instructions.\n";
        let p = parse(&to_toml(md, "reviewer"));
        assert_eq!(p.name, "reviewer");
        assert_eq!(p.description, "");
        assert_eq!(
            p.developer_instructions,
            "# Just a heading\n\nSome instructions."
        );
    }

    #[test]
    fn missing_name_field_falls_back_to_stem() {
        let md = "---\ndescription: A helper.\n---\nDo things.\n";
        let p = parse(&to_toml(md, "helper"));
        assert_eq!(p.name, "helper");
        assert_eq!(p.description, "A helper.");
    }

    #[test]
    fn quoted_scalar_is_unquoted() {
        let md = "---\nname: \"cto\"\ndescription: 'A quoted one.'\n---\nBody.\n";
        let p = parse(&to_toml(md, "x"));
        assert_eq!(p.name, "cto");
        assert_eq!(p.description, "A quoted one.");
    }

    #[test]
    fn output_is_deterministic() {
        let md = "---\nname: a\ndescription: d\n---\nbody with \"quotes\" and \\ backslash.\n";
        assert_eq!(to_toml(md, "a"), to_toml(md, "a"));
        // And it round-trips the tricky body intact.
        let p = parse(&to_toml(md, "a"));
        assert_eq!(
            p.developer_instructions,
            "body with \"quotes\" and \\ backslash."
        );
    }
}
