//! Managed-block contract for `markdown_section_sync` tools (Codex / Claude /
//! OpenClaw). This module currently implements the **read half** used by
//! `planner::inspect` to derive rule state from the instruction file. The write
//! half (`sync_markdown_rules`) lands with the apply pipeline.
//!
//! Markers are preserved verbatim from the VS Code extension for migration
//! parity. See `docs/tech/modules/rule-projection-sync.md`.

use std::fs;
use std::path::Path;

use crate::model::{CapabilityItem, RuleSyncError, RuleSyncOutcome};
use crate::paths::tildify;

pub const BLOCK_START: &str = "<!-- agentic-hub:start -->";
pub const BLOCK_END: &str = "<!-- agentic-hub:end -->";

const BLOCK_PREAMBLE: &str = "## Agentic Hub Managed Rules\n\nThis section is managed by Agentic Hub. Edit rule selections in the Capability Manager instead of editing these blocks by hand.";

/// State of the managed block within an instruction file's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockState {
    /// No markers present.
    Missing,
    /// One marker only, end before start, or duplicate markers.
    Malformed,
    /// Well-formed block; carries the inner content between the markers.
    Present(String),
}

/// Locate the managed block in a file's contents.
pub fn read_block(content: &str) -> BlockState {
    let start_count = content.matches(BLOCK_START).count();
    let end_count = content.matches(BLOCK_END).count();
    match (start_count, end_count) {
        (0, 0) => BlockState::Missing,
        (1, 1) => {
            let start = content.find(BLOCK_START).expect("counted one start");
            let end = content.find(BLOCK_END).expect("counted one end");
            if start < end {
                let inner = &content[start + BLOCK_START.len()..end];
                BlockState::Present(inner.to_string())
            } else {
                BlockState::Malformed
            }
        }
        _ => BlockState::Malformed,
    }
}

/// The managed block writes one `### <relative_path>` heading per enabled rule.
/// Membership is derived from that heading.
pub fn block_lists_rule(inner: &str, relative_path: &Path) -> bool {
    let rel = relative_path
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let heading = format!("### {rel}");
    inner.lines().any(|line| line.trim() == heading)
}

/// Strip a leading YAML frontmatter block (`---` … `---`). If none is present,
/// the whole content (trimmed) is the body.
pub fn strip_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start_matches(['\u{feff}', ' ', '\t', '\n', '\r']);
    let mut lines = trimmed.lines();
    if lines.next().map(str::trim) != Some("---") {
        return content.trim_end().to_string();
    }
    let mut body = String::new();
    let mut closed = false;
    for line in lines {
        if !closed {
            if line.trim() == "---" {
                closed = true;
            }
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    if closed {
        body.trim().to_string()
    } else {
        // No closing marker — treat the whole thing as body.
        content.trim_end().to_string()
    }
}

/// Build the full managed block (markers inclusive) for the enabled rules.
pub fn build_managed_block(enabled_rules: &[&CapabilityItem]) -> String {
    let mut out = String::new();
    out.push_str(BLOCK_START);
    out.push('\n');
    out.push_str(BLOCK_PREAMBLE);
    out.push_str("\n\n");
    for rule in enabled_rules {
        let rel = rule
            .relative_path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let body = fs::read_to_string(&rule.source_path)
            .map(|c| strip_frontmatter(&c))
            .unwrap_or_default();
        out.push_str(&format!("### {rel}\n\n"));
        out.push_str(&format!("Source: `{}`\n\n", tildify(&rule.source_path)));
        if !body.is_empty() {
            out.push_str(&body);
            out.push_str("\n\n");
        }
    }
    out.push_str(BLOCK_END);
    out
}

/// Write the managed rule block into `instructions`, preserving everything
/// outside the markers. Pure write-half of the `markdown_section_sync` contract.
pub fn sync_markdown_rules(
    instructions: &Path,
    enabled_rules: &[&CapabilityItem],
) -> Result<RuleSyncOutcome, RuleSyncError> {
    let err = |code: &str, msg: &str| RuleSyncError {
        path: instructions.to_path_buf(),
        code: code.to_string(),
        message: msg.to_string(),
    };

    if instructions.is_dir() {
        return Err(err(
            "conflict_real_file_at_target",
            "A directory occupies the instruction-file path",
        ));
    }

    let block = build_managed_block(enabled_rules);

    if !instructions.exists() {
        if enabled_rules.is_empty() {
            return Ok(RuleSyncOutcome::NoOp);
        }
        atomic_write(instructions, &format!("{block}\n"))
            .map_err(|e| err("internal", &e.to_string()))?;
        return Ok(RuleSyncOutcome::Wrote);
    }

    let content =
        fs::read_to_string(instructions).map_err(|e| err("permission_denied", &e.to_string()))?;

    match read_block(&content) {
        BlockState::Malformed => Err(err(
            "rule_sync_malformed_markers",
            "Instruction file has malformed managed-block markers",
        )),
        BlockState::Missing => {
            if enabled_rules.is_empty() {
                return Ok(RuleSyncOutcome::NoOp);
            }
            let joined = format!("{}\n\n{block}\n", content.trim_end());
            atomic_write(instructions, &joined).map_err(|e| err("internal", &e.to_string()))?;
            Ok(RuleSyncOutcome::Wrote)
        }
        BlockState::Present(_) => {
            let start = content.find(BLOCK_START).expect("present block has start");
            let end = content.find(BLOCK_END).expect("present block has end") + BLOCK_END.len();
            let before = &content[..start];
            let after = &content[end..];

            if enabled_rules.is_empty() {
                let remainder = format!("{}{}", before.trim_end(), after);
                if remainder.trim().is_empty() {
                    fs::remove_file(instructions).map_err(|e| err("internal", &e.to_string()))?;
                    return Ok(RuleSyncOutcome::Removed);
                }
                atomic_write(instructions, &format!("{}\n", remainder.trim_end()))
                    .map_err(|e| err("internal", &e.to_string()))?;
                return Ok(RuleSyncOutcome::Wrote);
            }

            let rebuilt = format!("{before}{block}{after}");
            atomic_write(instructions, &rebuilt).map_err(|e| err("internal", &e.to_string()))?;
            Ok(RuleSyncOutcome::Wrote)
        }
    }
}

/// Atomic write (`.tmp` + rename), falling back to a direct write if the rename
/// crosses a filesystem boundary.
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("agentic-rules.tmp");
    if fs::write(&tmp, content).is_ok() && fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&tmp);
    fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn block(rules: &[&str]) -> String {
        let mut s = String::from("# Instructions\n\nsome user content\n\n");
        s.push_str(BLOCK_START);
        s.push('\n');
        for r in rules {
            s.push_str(&format!(
                "### {r}\n\nSource: `~/.agentic/rules/{r}`\n\nbody\n\n"
            ));
        }
        s.push_str(BLOCK_END);
        s.push('\n');
        s
    }

    #[test]
    fn missing_block() {
        assert_eq!(read_block("no markers here"), BlockState::Missing);
    }

    #[test]
    fn malformed_variants() {
        assert_eq!(read_block(BLOCK_START), BlockState::Malformed);
        assert_eq!(
            read_block(&format!("{BLOCK_END}\n{BLOCK_START}")),
            BlockState::Malformed
        );
        assert_eq!(
            read_block(&format!("{BLOCK_START}{BLOCK_START}{BLOCK_END}")),
            BlockState::Malformed
        );
    }

    #[test]
    fn present_lists_rules() {
        let content = block(&["general/precise.mdc", "frontend/components.md"]);
        let BlockState::Present(inner) = read_block(&content) else {
            panic!("expected present block");
        };
        assert!(block_lists_rule(&inner, Path::new("general/precise.mdc")));
        assert!(block_lists_rule(
            &inner,
            Path::new("frontend/components.md")
        ));
        assert!(!block_lists_rule(&inner, Path::new("general/missing.mdc")));
    }
}
