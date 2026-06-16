//! CLI tool preflight: a catalog of high-frequency command-line tools the user
//! likely needs before building agentic systems (Node, Python, Homebrew, git,
//! the GitHub/GitLab CLIs, the agent CLIs, Vercel), plus a probe that reports
//! whether each is installed and (where it matters) authenticated.
//!
//! The catalog is **data**: a bundled JSON shipped in the repo, optionally
//! merged with a user-local override (and, later, a remote source). Each tool
//! declares a `check` command (installed + version) and an optional `auth`
//! command (authenticated or not). Probing shells out with the resolved login
//! `PATH` (see [`crate::shell_env`]) so Dock-launched GUIs still find binaries.
//!
//! Security: probe commands run as `program` + explicit `args` — never through
//! a shell — so they can't carry metacharacters. The bundled catalog is
//! trusted; a user-local override runs the user's own commands on their own
//! machine (analogous to the existing custom source paths). See
//! `docs/tech/modules/cli-tools.md`.

use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// The bundled tool catalog, embedded from `resources/cli-tools/catalog.json`
/// at the repo root so there is no network fetch and no missing-file failure.
const BUNDLED_CATALOG: &str = include_str!("../../../resources/cli-tools/catalog.json");

/// How long a single probe command may run before it is killed. Auth checks can
/// touch the network (token validation), so this guards the blocking pool.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// A concrete command to run: a program and its explicit argument vector. Run
/// directly (never via a shell), so args can't be reinterpreted.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// One catalog entry: how to identify, check, install, and (optionally)
/// auth-check a CLI tool.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliTool {
    /// Stable id (e.g. `"gh"`). Used to merge overrides and address a row.
    pub id: String,
    /// Human-readable name (e.g. `"GitHub CLI"`).
    pub name: String,
    /// Optional grouping label for future sectioning.
    #[serde(default)]
    pub category: Option<String>,
    /// Web page documenting how to install the tool. Opened in the browser.
    pub install_url: String,
    /// Command that succeeds when the tool is installed; its stdout yields the
    /// version line.
    pub check: CliCommand,
    /// Optional command that succeeds only when the tool is authenticated.
    /// `None` for tools that need no auth.
    #[serde(default)]
    pub auth: Option<CliCommand>,
}

/// Authentication state of a tool. `notApplicable` covers both "no auth needed"
/// and "not installed, so auth is moot".
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthState {
    /// The tool declares no auth, or it is not installed.
    NotApplicable,
    /// The auth check succeeded.
    Authed,
    /// The auth check ran but reported the tool is not signed in.
    NotAuthed,
    /// The auth check could not be completed (timed out / failed to spawn).
    Unknown,
}

/// Result of probing one tool. Advisory snapshot surfaced in the Tools table.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliToolStatus {
    pub id: String,
    pub installed: bool,
    #[serde(default)]
    pub version: Option<String>,
    pub auth: AuthState,
    /// Short human note (e.g. when a probe timed out).
    #[serde(default)]
    pub message: Option<String>,
}

/// Parse a catalog JSON document into tools. Malformed JSON is a typed error so
/// a bad user override never silently yields an empty list.
pub fn parse_catalog(json: &str) -> Result<Vec<CliTool>> {
    serde_json::from_str(json)
        .map_err(|e| CoreError::SettingsParse(format!("malformed CLI tools catalog: {e}")))
}

/// The bundled catalog, parsed. Panics only if the embedded JSON is malformed,
/// which a unit test guards against — so it can never happen in a shipped build.
pub fn bundled_catalog() -> Result<Vec<CliTool>> {
    parse_catalog(BUNDLED_CATALOG)
}

/// Merge `custom` over `bundled`: a custom entry with the same `id` replaces the
/// bundled one in place (preserving order); a new id is appended. Lets a user
/// override a single tool's commands without restating the whole catalog.
pub fn merge_catalogs(bundled: Vec<CliTool>, custom: Vec<CliTool>) -> Vec<CliTool> {
    let mut out = bundled;
    for tool in custom {
        if let Some(existing) = out.iter_mut().find(|t| t.id == tool.id) {
            *existing = tool;
        } else {
            out.push(tool);
        }
    }
    out
}

/// Extract a version string from a probe's stdout: the first non-empty trimmed
/// line. Tools format versions wildly (`v20.10.0`, `git version 2.39`,
/// `Python 3.11.5`), so we surface the raw first line rather than guess a
/// canonical shape.
pub fn parse_version(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Outcome of a finished probe command. `None` from [`run_with_timeout`] means
/// the command was killed for exceeding [`PROBE_TIMEOUT`].
struct ProbeOutput {
    success: bool,
    stdout: String,
}

/// Run `program`/`args` with the login `PATH`, killing it after [`PROBE_TIMEOUT`].
/// `Ok(None)` = timed out; `Err` = failed to spawn (e.g. program not found).
fn run_with_timeout(command: &CliCommand) -> std::io::Result<Option<ProbeOutput>> {
    let mut cmd = crate::shell_env::command_with_login_path(&command.program);
    // stderr is discarded, not piped: we never read it, and an undrained stderr
    // pipe would deadlock a tool that writes more than the OS buffer there
    // before exiting (e.g. `gh auth status` prints to stderr). We only consume
    // stdout (version) and the exit status (success).
    cmd.args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                // The child has exited; drain its (small) piped output. try_wait
                // already reaped it, so we read the handles directly rather than
                // calling wait_with_output (which would double-wait).
                let mut stdout = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout);
                }
                return Ok(Some(ProbeOutput {
                    success: status.success(),
                    stdout,
                }));
            }
            None => {
                if start.elapsed() >= PROBE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Probe one tool: run its `check` (installed + version), then its `auth` when
/// installed and declared. Impure (shells out); the parsing it relies on is the
/// unit-tested [`parse_version`].
pub fn check_tool(tool: &CliTool) -> CliToolStatus {
    let probe = match run_with_timeout(&tool.check) {
        Ok(Some(p)) => p,
        // Not found / failed to spawn → not installed (the common case).
        Ok(None) => {
            return CliToolStatus {
                id: tool.id.clone(),
                installed: false,
                version: None,
                auth: AuthState::NotApplicable,
                message: Some("Check timed out.".to_string()),
            };
        }
        Err(_) => {
            return CliToolStatus {
                id: tool.id.clone(),
                installed: false,
                version: None,
                auth: AuthState::NotApplicable,
                message: None,
            };
        }
    };

    if !probe.success {
        return CliToolStatus {
            id: tool.id.clone(),
            installed: false,
            version: None,
            auth: AuthState::NotApplicable,
            message: None,
        };
    }

    let version = parse_version(&probe.stdout);
    let auth = match &tool.auth {
        None => AuthState::NotApplicable,
        Some(cmd) => match run_with_timeout(cmd) {
            Ok(Some(p)) if p.success => AuthState::Authed,
            Ok(Some(_)) => AuthState::NotAuthed,
            // Timed out or failed to spawn the auth checker.
            Ok(None) | Err(_) => AuthState::Unknown,
        },
    };

    CliToolStatus {
        id: tool.id.clone(),
        installed: true,
        version,
        auth,
        message: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_parses_and_has_expected_tools() {
        let tools = bundled_catalog().expect("bundled catalog must be valid JSON");
        let ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        for expected in [
            "node",
            "python3",
            "homebrew",
            "git",
            "gh",
            "glab",
            "claude",
            "codex",
            "cursor-agent",
            "vercel",
        ] {
            assert!(ids.contains(&expected), "catalog missing tool: {expected}");
        }
        // Every tool ships a non-empty check program and an install URL.
        for t in &tools {
            assert!(!t.check.program.is_empty(), "{} has empty check", t.id);
            assert!(
                t.install_url.starts_with("http"),
                "{} bad install url",
                t.id
            );
        }
    }

    #[test]
    fn parse_catalog_rejects_malformed_json() {
        let err = parse_catalog("not json").unwrap_err();
        assert!(matches!(err, CoreError::SettingsParse(_)));
    }

    #[test]
    fn parse_version_takes_first_non_empty_line() {
        assert_eq!(parse_version("v20.10.0\n").as_deref(), Some("v20.10.0"));
        assert_eq!(
            parse_version("\n  git version 2.39.3  \nextra").as_deref(),
            Some("git version 2.39.3")
        );
        assert_eq!(parse_version("   \n\n").as_deref(), None);
        assert_eq!(parse_version("").as_deref(), None);
    }

    #[test]
    fn merge_catalogs_overrides_by_id_and_appends_new() {
        let bundled = vec![
            CliTool {
                id: "git".into(),
                name: "Git".into(),
                category: None,
                install_url: "https://git-scm.com".into(),
                check: CliCommand {
                    program: "git".into(),
                    args: vec!["--version".into()],
                },
                auth: None,
            },
            CliTool {
                id: "gh".into(),
                name: "GitHub CLI".into(),
                category: None,
                install_url: "https://cli.github.com".into(),
                check: CliCommand {
                    program: "gh".into(),
                    args: vec!["--version".into()],
                },
                auth: None,
            },
        ];
        let custom = vec![
            // Override git with a different check program.
            CliTool {
                id: "git".into(),
                name: "Git (custom)".into(),
                category: None,
                install_url: "https://example.com".into(),
                check: CliCommand {
                    program: "mygit".into(),
                    args: vec![],
                },
                auth: None,
            },
            // A brand-new tool.
            CliTool {
                id: "fly".into(),
                name: "Fly.io".into(),
                category: None,
                install_url: "https://fly.io".into(),
                check: CliCommand {
                    program: "flyctl".into(),
                    args: vec!["version".into()],
                },
                auth: None,
            },
        ];
        let merged = merge_catalogs(bundled, custom);
        assert_eq!(merged.len(), 3, "git replaced in place, fly appended");
        // Order preserved: git, gh, fly.
        assert_eq!(merged[0].id, "git");
        assert_eq!(merged[0].name, "Git (custom)");
        assert_eq!(merged[0].check.program, "mygit");
        assert_eq!(merged[1].id, "gh");
        assert_eq!(merged[2].id, "fly");
    }

    #[test]
    fn check_tool_reports_not_installed_for_missing_program() {
        let tool = CliTool {
            id: "ghost".into(),
            name: "Ghost".into(),
            category: None,
            install_url: "https://example.com".into(),
            check: CliCommand {
                program: "definitely-not-a-real-binary-xyz-12345".into(),
                args: vec!["--version".into()],
            },
            auth: Some(CliCommand {
                program: "definitely-not-a-real-binary-xyz-12345".into(),
                args: vec!["auth".into()],
            }),
        };
        let status = check_tool(&tool);
        assert_eq!(status.id, "ghost");
        assert!(!status.installed);
        assert!(status.version.is_none());
        // Auth is moot for an uninstalled tool.
        assert_eq!(status.auth, AuthState::NotApplicable);
    }

    // The probe branches below shell out to ubiquitous POSIX utilities
    // (`echo`/`true`/`false`) so the installed/version/auth paths are exercised
    // deterministically. Unix-only: these binaries don't exist on Windows.
    #[cfg(unix)]
    fn probe_tool(check: CliCommand, auth: Option<CliCommand>) -> CliTool {
        CliTool {
            id: "probe".into(),
            name: "Probe".into(),
            category: None,
            install_url: "https://example.com".into(),
            check,
            auth,
        }
    }

    #[cfg(unix)]
    #[test]
    fn check_tool_captures_version_for_installed_tool_without_auth() {
        let tool = probe_tool(
            CliCommand {
                program: "echo".into(),
                args: vec!["v9.9.9".into()],
            },
            None,
        );
        let status = check_tool(&tool);
        assert!(status.installed);
        assert_eq!(status.version.as_deref(), Some("v9.9.9"));
        assert_eq!(status.auth, AuthState::NotApplicable);
    }

    #[cfg(unix)]
    #[test]
    fn check_tool_reports_authed_when_auth_command_succeeds() {
        let tool = probe_tool(
            CliCommand {
                program: "true".into(),
                args: vec![],
            },
            Some(CliCommand {
                program: "true".into(),
                args: vec![],
            }),
        );
        let status = check_tool(&tool);
        assert!(status.installed);
        assert_eq!(status.auth, AuthState::Authed);
    }

    #[cfg(unix)]
    #[test]
    fn check_tool_reports_not_authed_when_auth_command_fails() {
        let tool = probe_tool(
            CliCommand {
                program: "true".into(),
                args: vec![],
            },
            Some(CliCommand {
                program: "false".into(),
                args: vec![],
            }),
        );
        let status = check_tool(&tool);
        assert!(status.installed);
        assert_eq!(status.auth, AuthState::NotAuthed);
    }

    #[cfg(unix)]
    #[test]
    fn check_tool_reports_unknown_when_auth_command_cannot_spawn() {
        let tool = probe_tool(
            CliCommand {
                program: "true".into(),
                args: vec![],
            },
            Some(CliCommand {
                program: "definitely-not-a-real-binary-xyz-12345".into(),
                args: vec!["status".into()],
            }),
        );
        let status = check_tool(&tool);
        assert!(status.installed);
        assert_eq!(status.auth, AuthState::Unknown);
    }
}
