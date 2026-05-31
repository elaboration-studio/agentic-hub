//! Managed-block contract for `markdown_section_sync` tools (Codex / Claude /
//! OpenClaw). This module currently implements the **read half** used by
//! `planner::inspect` to derive rule state from the instruction file. The write
//! half (`sync_markdown_rules`) lands with the apply pipeline.
//!
//! Markers are preserved verbatim from the VS Code extension for migration
//! parity. See `docs/tech/modules/rule-projection-sync.md`.

use std::path::Path;

pub const BLOCK_START: &str = "<!-- e-studio-agentic-rules:start -->";
pub const BLOCK_END: &str = "<!-- e-studio-agentic-rules:end -->";

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
