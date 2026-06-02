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
    use crate::model::CapabilityKind;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn rule_item(rel: &str, source: PathBuf) -> CapabilityItem {
        CapabilityItem {
            id: format!("rule:{rel}"),
            kind: CapabilityKind::Rule,
            name: rel.to_string(),
            source_path: source,
            relative_path: PathBuf::from(rel),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            valid: true,
            validation_errors: vec![],
        }
    }

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

    // ---- strip_frontmatter --------------------------------------------------

    #[test]
    fn strip_frontmatter_handles_all_shapes() {
        // Well-formed frontmatter: only the body survives.
        assert_eq!(
            strip_frontmatter("---\nname: x\n---\n\nbody line\n"),
            "body line"
        );
        // BOM + leading whitespace before the opening marker is tolerated.
        assert_eq!(
            strip_frontmatter("\u{feff}\n---\nk: v\n---\nbody\n"),
            "body"
        );
        // No frontmatter: the whole (trimmed) content is the body.
        assert_eq!(strip_frontmatter("just a body\n\n"), "just a body");
        // Unclosed frontmatter: treated as a plain body, kept verbatim (trimmed).
        assert_eq!(
            strip_frontmatter("---\nname: x\nstill open"),
            "---\nname: x\nstill open"
        );
    }

    // ---- build_managed_block ------------------------------------------------

    #[test]
    fn build_managed_block_emits_heading_source_and_body() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("rules/general/precise.mdc");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "---\nname: precise\n---\n\nbe precise").unwrap();
        let rule = rule_item("general/precise.mdc", source);

        let out = build_managed_block(&[&rule]);
        assert!(out.starts_with(BLOCK_START));
        assert!(out.ends_with(BLOCK_END));
        assert!(out.contains("### general/precise.mdc"));
        assert!(out.contains("Source: `"));
        assert!(out.contains("be precise"), "frontmatter stripped from body");
        assert!(!out.contains("name: precise"), "frontmatter not emitted");
    }

    #[test]
    fn build_managed_block_missing_source_yields_empty_body() {
        let rule = rule_item("gone.mdc", PathBuf::from("/no/such/file.mdc"));
        let out = build_managed_block(&[&rule]);
        assert!(out.contains("### gone.mdc"));
        // No panic, and no body text beyond the heading + source line.
        assert!(out.contains(BLOCK_END));
    }

    // ---- sync_markdown_rules (write half) ----------------------------------

    fn instr(dir: &Path) -> PathBuf {
        dir.join("AGENTS.md")
    }

    #[test]
    fn sync_creates_file_when_absent_with_rules() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("r.mdc");
        fs::write(&src, "rule body").unwrap();
        let rule = rule_item("r.mdc", src);
        let path = instr(dir.path());

        let outcome = sync_markdown_rules(&path, &[&rule]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::Wrote);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(BLOCK_START));
        assert!(written.contains("### r.mdc"));
        assert!(written.contains("rule body"));
    }

    #[test]
    fn sync_absent_file_with_no_rules_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        let outcome = sync_markdown_rules(&path, &[]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::NoOp);
        assert!(!path.exists());
    }

    #[test]
    fn sync_appends_block_preserving_prior_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        fs::write(&path, "# My Agents\n\nhand-written guidance\n").unwrap();
        let src = dir.path().join("r.mdc");
        fs::write(&src, "rule body").unwrap();
        let rule = rule_item("r.mdc", src);

        let outcome = sync_markdown_rules(&path, &[&rule]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::Wrote);
        let written = fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("hand-written guidance"),
            "prior content kept"
        );
        assert!(written.contains(BLOCK_START));
        assert!(written.contains("### r.mdc"));
    }

    #[test]
    fn sync_rebuilds_present_block_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        // Seed a file that already has a managed block listing an old rule.
        fs::write(&path, block(&["old.mdc"])).unwrap();
        let src = dir.path().join("new.mdc");
        fs::write(&src, "new body").unwrap();
        let rule = rule_item("new.mdc", src);

        let outcome = sync_markdown_rules(&path, &[&rule]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::Wrote);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("# Instructions"), "surrounding text kept");
        assert!(written.contains("### new.mdc"));
        assert!(!written.contains("### old.mdc"), "old rule replaced");
        assert_eq!(written.matches(BLOCK_START).count(), 1, "single block");
    }

    #[test]
    fn sync_empty_rules_removes_hooks_only_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        let only_block = format!("{BLOCK_START}\n### r.mdc\n\nbody\n{BLOCK_END}\n");
        fs::write(&path, only_block).unwrap();

        let outcome = sync_markdown_rules(&path, &[]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::Removed);
        assert!(!path.exists(), "file deleted once nothing remains");
    }

    #[test]
    fn sync_empty_rules_strips_block_but_keeps_surrounding() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        fs::write(&path, block(&["r.mdc"])).unwrap();

        let outcome = sync_markdown_rules(&path, &[]).unwrap();
        assert_eq!(outcome, RuleSyncOutcome::Wrote);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("some user content"), "user content kept");
        assert!(!written.contains(BLOCK_START), "managed block removed");
    }

    #[test]
    fn sync_malformed_markers_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        fs::write(&path, format!("text\n{BLOCK_START}\nno end marker")).unwrap();
        let src = dir.path().join("r.mdc");
        fs::write(&src, "body").unwrap();
        let rule = rule_item("r.mdc", src);

        let err = sync_markdown_rules(&path, &[&rule]).unwrap_err();
        assert_eq!(err.code, "rule_sync_malformed_markers");
    }

    #[test]
    fn sync_directory_at_target_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = instr(dir.path());
        fs::create_dir_all(&path).unwrap();
        let err = sync_markdown_rules(&path, &[]).unwrap_err();
        assert_eq!(err.code, "conflict_real_file_at_target");
    }
}
