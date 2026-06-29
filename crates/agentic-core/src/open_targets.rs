//! Path-allowlist for the "open / reveal file" affordance. The WebView never
//! gets raw FS or opener scope; instead the shell calls a command that asks
//! this module whether a path is openable. A path is allowed only when it
//! resolves under a configured source root, a configured tool target path, or
//! a known workspace directory. See `ARCHITECTURE.permissions.md`.

use std::path::{Path, PathBuf};

use crate::settings::{Settings, ToolSettings};

/// True if `candidate` is safe to open: it exists and canonicalizes to a path
/// inside a configured source root, a tool target dir, an exact tool file
/// (instruction / hooks file), or a known workspace directory.
pub fn is_openable(candidate: &Path, settings: &Settings, workspace_dirs: &[PathBuf]) -> bool {
    let Ok(real) = candidate.canonicalize() else {
        return false;
    };

    let mut dir_roots: Vec<PathBuf> = Vec::new();
    let mut exact_files: Vec<PathBuf> = Vec::new();

    for source in settings.resolve_sources() {
        dir_roots.push(source.path);
    }
    for tool in [
        &settings.tools.codex,
        &settings.tools.claude,
        &settings.tools.cursor,
        &settings.tools.openclaw,
        &settings.tools.openstandard,
        &settings.tools.kiro,
        &settings.tools.copilot,
        &settings.tools.antigravity,
    ] {
        collect_tool_targets(tool, &mut dir_roots, &mut exact_files);
    }
    dir_roots.extend(workspace_dirs.iter().cloned());

    // Directory roots: candidate is allowed if it is the root or lives under it.
    for root in dir_roots {
        if let Ok(real_root) = root.canonicalize() {
            if real == real_root || real.starts_with(&real_root) {
                return true;
            }
        }
    }
    // Exact files (instruction / hooks files): candidate must equal them.
    for file in exact_files {
        if let Ok(real_file) = file.canonicalize() {
            if real == real_file {
                return true;
            }
        }
    }
    false
}

/// True if `url` is safe to hand to the system browser: an absolute `http`/
/// `https` URL with a host. Everything else (no scheme, `file:`, `javascript:`,
/// `mailto:`, scheme-relative `//host`, …) is rejected so the open affordance
/// can never be coerced into running a local handler from WebView-supplied data.
pub fn is_safe_external_url(url: &str) -> bool {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    // Reject `https:///path` and `https://` — require a non-empty host.
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty()
}

fn collect_tool_targets(tool: &ToolSettings, dirs: &mut Vec<PathBuf>, files: &mut Vec<PathBuf>) {
    dirs.push(tool.skills_path.clone());
    dirs.push(tool.agents_path.clone());
    dirs.push(tool.rules_path.clone());
    if let Some(p) = &tool.commands_path {
        dirs.push(p.clone());
    }
    if let Some(p) = &tool.instructions_path {
        files.push(p.clone());
    }
    if let Some(p) = &tool.hooks_file {
        files.push(p.clone());
    }
    if let Some(p) = &tool.hooks_dir {
        dirs.push(p.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn allows_files_under_a_source_root() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let file = root.path().join("skills/a/SKILL.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "# a").unwrap();

        assert!(is_openable(&file, &settings, &[]));
    }

    #[test]
    fn allows_projected_file_under_a_tool_dir() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let projected = settings.tools.codex.skills_path.join("a");
        fs::create_dir_all(&projected).unwrap();
        assert!(is_openable(&projected, &settings, &[]));
    }

    #[test]
    fn allows_projected_command_file() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let cmd = settings
            .tools
            .codex
            .commands_path
            .clone()
            .unwrap()
            .join("review/code-review.md");
        fs::create_dir_all(cmd.parent().unwrap()).unwrap();
        fs::write(&cmd, "# review").unwrap();
        assert!(is_openable(&cmd, &settings, &[]));
    }

    #[test]
    fn allows_exact_instruction_file() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let instr = settings.tools.codex.instructions_path.clone().unwrap();
        fs::create_dir_all(instr.parent().unwrap()).unwrap();
        fs::write(&instr, "rules").unwrap();
        assert!(is_openable(&instr, &settings, &[]));
    }

    #[test]
    fn allows_files_under_a_known_workspace_dir() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let file = ws.path().join(".cursor/rules/x.mdc");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x").unwrap();
        assert!(is_openable(&file, &settings, &[ws.path().to_path_buf()]));
    }

    #[test]
    fn rejects_paths_outside_every_root() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());

        let file = outside.path().join("secret.txt");
        fs::write(&file, "nope").unwrap();
        assert!(!is_openable(&file, &settings, &[]));
    }

    #[test]
    fn rejects_nonexistent_path() {
        let root = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let settings = Settings::sandboxed(root.path(), tools.path());
        assert!(!is_openable(&root.path().join("nope"), &settings, &[]));
    }

    #[test]
    fn accepts_http_and_https_urls_with_a_host() {
        assert!(is_safe_external_url(
            "https://skills.sh/skills/openhands/skills"
        ));
        assert!(is_safe_external_url("http://github.com/owner/repo"));
        assert!(is_safe_external_url("  https://skills.sh  "));
    }

    #[test]
    fn rejects_non_http_schemes_and_malformed_urls() {
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("mailto:a@b.com"));
        assert!(!is_safe_external_url("//skills.sh"));
        assert!(!is_safe_external_url("skills.sh"));
        assert!(!is_safe_external_url("https://"));
        assert!(!is_safe_external_url("https:///path"));
        assert!(!is_safe_external_url(""));
    }
}
