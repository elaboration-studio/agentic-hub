//! Pluggable public skill sources and the install path they drive.
//!
//! A [`SkillProvider`] knows how to check its tooling and install a skill into a
//! project directory. Today there is one provider, [`SkillsShProvider`], which
//! shells out to the `skills` CLI via `npx`. The seam exists so other public
//! sources (other registries, other CLIs) can be added without touching the
//! command layer.
//!
//! This is the **only** workspace write path. It is invoked explicitly by the
//! user from Workspace scope; the inventory scan itself stays read-only. See
//! `docs/tech/modules/skill-sources.md`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::ToolId;

/// Public, keyless base for the skills.sh search index — the same endpoint the
/// official `skills` CLI uses (`GET /api/search`). The `/api/v1/*` endpoints are
/// key-gated and deliberately not used here.
const SKILLS_SH_SEARCH_URL: &str = "https://skills.sh/api/search";

/// Result of probing a provider's CLI tooling. Advisory: surfaced in Config so
/// the user knows whether installs will work before trying.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCliStatus {
    pub available: bool,
    #[serde(default)]
    pub version: Option<String>,
    pub message: String,
}

/// Outcome of an install run. `ok` mirrors the child process exit status; `log`
/// is the combined stdout/stderr (trimmed) for display. The authoritative view
/// of what landed is the subsequent workspace re-scan, not this struct.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallResult {
    pub ok: bool,
    pub log: String,
}

/// A streamed install event, sent one-per-output-line over a Tauri channel and
/// terminated by a single `done`. Mirrors the shape the install window's live
/// console consumes; tagged by `kind` so the UI can switch on it.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SkillInstallEvent {
    /// One line of process output. `stream` is `"stdout"` or `"stderr"`.
    Line { stream: String, text: String },
    /// Terminal event: the run finished. `ok` mirrors the exit status;
    /// `cancelled` is true when the user killed it mid-run.
    Done { ok: bool, cancelled: bool },
}

/// One skill returned by a provider's search. Mirrors the keyless skills.sh
/// search shape plus links derived once in Rust so the UI never reconstructs
/// URLs: `install_ref` is the `owner/repo` the CLI accepts, `github_url` is the
/// source repo (absent for non-GitHub sources), `page_url` is the skills.sh page.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSearchHit {
    /// Stable `{source}/{slug}` identifier.
    pub id: String,
    /// URL-safe skill slug (the per-skill name within its source).
    pub skill_id: String,
    /// Human-readable name.
    pub name: String,
    /// Source repo/provider, e.g. `owner/repo`.
    pub source: String,
    /// Total deduplicated install count.
    pub installs: u32,
    /// The ref to hand `npx skills add` — the source today.
    pub install_ref: String,
    /// Source repo URL, when the source is a GitHub `owner/repo`.
    pub github_url: Option<String>,
    /// The skill's page on skills.sh.
    pub page_url: String,
}

/// A public skill source capable of checking its tooling and installing into a
/// project. Keep implementations thin: validation and arg-building are pure and
/// unit-tested; only the actual process spawn / network call is impure.
pub trait SkillProvider {
    /// Stable provider id, e.g. `"skills.sh"`.
    fn id(&self) -> &'static str;
    /// Probe whether this provider's installer can run on this machine.
    fn cli_check(&self) -> SkillCliStatus;
    /// Search the provider's public index for `query`. Returns at most `limit`
    /// hits; queries under two characters return an empty list without a call.
    fn search(&self, query: &str, limit: u32) -> Result<Vec<SkillSearchHit>>;
    /// Install `install_ref` into `workspace_dir`. `skill` is the one slug to
    /// install (a multi-skill repo otherwise prompts an interactive picker);
    /// `tools` is the user's target selection, mapped to explicit `--agent`
    /// flags so the CLI never prompts for agents either.
    fn install(
        &self,
        workspace_dir: &Path,
        install_ref: &str,
        skill: Option<&str>,
        tools: &[ToolId],
    ) -> Result<SkillInstallResult>;
}

/// Resolve a provider by id. Returns `None` for unknown providers so the command
/// layer can surface a typed error.
pub fn provider_for(id: &str) -> Option<Box<dyn SkillProvider>> {
    match id {
        "skills.sh" => Some(Box::new(SkillsShProvider)),
        _ => None,
    }
}

/// True when `r` is a safe `owner/repo` (optionally `owner/repo/skill`) ref:
/// only `[A-Za-z0-9._-]` per component, at least one `/`, and no empty, `.`, or
/// `..` components. This is the injection guard before the ref reaches a process
/// arg — it can never carry shell metacharacters or path traversal.
pub fn validate_install_ref(r: &str) -> bool {
    let parts: Vec<&str> = r.split('/').collect();
    if parts.len() < 2 {
        return false;
    }
    parts.iter().all(|p| {
        !p.is_empty()
            && *p != "."
            && *p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

/// The skills.sh CLI's `--agent` identifier for a tool. The CLI names Claude
/// `claude-code`; the others match our `ToolId`. Kept here rather than on
/// `ToolId` because the naming is provider-specific.
fn skills_agent_id(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Codex => "codex",
        ToolId::Claude => "claude-code",
        ToolId::Cursor => "cursor",
        ToolId::Openclaw => "openclaw",
        // OpenStandard is global-only; skills.sh install is workspace-only, so
        // this arm is effectively unreachable. It maps to its raw id.
        ToolId::Openstandard => "openstandard",
    }
}

/// True when `s` is a safe skill slug: non-empty, only `[A-Za-z0-9._-]`, and not
/// leading with `-` (so it can never be read as a CLI flag). Like the ref guard,
/// this runs before the slug becomes a process arg.
pub fn validate_skill_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The full, non-interactive `npx` argument vector for installing a single
/// (pre-validated) skill. Kept pure so the exact invocation is asserted in
/// tests. The leading `--yes` is npx's (fetch `skills@latest` without a prompt);
/// `--skill <slug>` pins the one starred skill — a multi-skill repo otherwise
/// opens an interactive picker that hangs a headless run; one `--agent <id>` per
/// selected tool targets exactly those agents (else the CLI prompts for agents);
/// the trailing `--yes` skips the skills CLI's own confirmation. Callers must
/// `validate_install_ref` / `validate_skill_slug` first.
pub fn skills_npx_args(install_ref: &str, skill: Option<&str>, tools: &[ToolId]) -> Vec<String> {
    let mut args = vec![
        "--yes".to_string(),
        "skills@latest".to_string(),
        "add".to_string(),
        install_ref.to_string(),
    ];
    if let Some(slug) = skill {
        args.push("--skill".to_string());
        args.push(slug.to_string());
    }
    for tool in tools {
        args.push("--agent".to_string());
        args.push(skills_agent_id(*tool).to_string());
    }
    args.push("--yes".to_string());
    args
}

/// Build the `npx skills add …` command with the resolved login `PATH` and
/// telemetry disabled — ready for the caller to set `current_dir`, pipe stdio,
/// and spawn. Pure assembly (no spawn) so the streaming shell layer owns IO; the
/// exact program + arg vector is asserted in tests. Callers must
/// `validate_install_ref` / `validate_skill_slug` first — this does not re-check.
pub fn skills_install_command(install_ref: &str, skill: Option<&str>, tools: &[ToolId]) -> Command {
    let mut cmd = npx_command();
    cmd.args(skills_npx_args(install_ref, skill, tools));
    cmd
}

/// The non-interactive `npx` argument vector for updating one already-installed
/// (pre-validated) skill in the current project. `--project` pins project scope
/// (the skill lives in `skills-lock.json` here, not the global lock) and `--yes`
/// skips the scope prompt; the leading `--yes` is npx's. The skill name is the
/// lock key. Callers must `validate_skill_slug` first.
pub fn skills_update_npx_args(skill: &str) -> Vec<String> {
    vec![
        "--yes".to_string(),
        "skills@latest".to_string(),
        "update".to_string(),
        skill.to_string(),
        "--project".to_string(),
        "--yes".to_string(),
    ]
}

/// Build the `npx skills update <name> --project --yes` command with the login
/// `PATH` and telemetry disabled — ready for the caller to set `current_dir`,
/// pipe stdio, and spawn. Pure assembly (no spawn); the exact program + argv is
/// asserted in tests. Callers must `validate_skill_slug` first.
pub fn skills_update_command(skill: &str) -> Command {
    let mut cmd = npx_command();
    cmd.args(skills_update_npx_args(skill));
    cmd
}

/// Percent-encode a query component (RFC 3986 unreserved set passes through).
fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The exact keyless search URL for `query`/`limit`. Pure and unit-tested so the
/// request shape can't drift from what the skills.sh index expects.
pub fn search_url(query: &str, limit: u32) -> String {
    format!(
        "{SKILLS_SH_SEARCH_URL}?q={}&limit={}",
        encode_query(query.trim()),
        limit
    )
}

/// Raw wire shape of the keyless `/api/search` response. Kept private; callers
/// see the enriched [`SkillSearchHit`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSearchResponse {
    #[serde(default)]
    skills: Vec<RawSearchHit>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSearchHit {
    id: String,
    #[serde(default)]
    skill_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    installs: u32,
}

/// Enrich a raw hit with the links the UI needs. `github_url` is only set when
/// the source is an `owner/repo` (GitHub) ref, not a well-known domain source.
fn enrich_hit(raw: RawSearchHit) -> SkillSearchHit {
    let github_url = raw
        .source
        .contains('/')
        .then(|| format!("https://github.com/{}", raw.source));
    SkillSearchHit {
        page_url: format!("https://skills.sh/{}", raw.id),
        install_ref: raw.source.clone(),
        github_url,
        id: raw.id,
        skill_id: raw.skill_id,
        name: raw.name,
        source: raw.source,
        installs: raw.installs,
    }
}

/// Parse a keyless search response body into enriched hits. Pure and tested.
pub fn parse_search_response(body: &str) -> Result<Vec<SkillSearchHit>> {
    let raw: RawSearchResponse = serde_json::from_str(body)
        .map_err(|e| CoreError::SkillSearch(format!("malformed search response: {e}")))?;
    Ok(raw.skills.into_iter().map(enrich_hit).collect())
}

/// Blocking GET of the keyless search index. The only impure part of search.
fn http_search(query: &str, limit: u32) -> Result<Vec<SkillSearchHit>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("agentic-hub")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| CoreError::SkillSearch(e.to_string()))?;
    let resp = client
        .get(search_url(query, limit))
        .send()
        .map_err(|e| CoreError::SkillSearch(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(CoreError::SkillSearch(format!(
            "skills.sh returned {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .map_err(|e| CoreError::SkillSearch(e.to_string()))?;
    parse_search_response(&body)
}

/// Build an `npx` command with the login `PATH` applied and telemetry disabled.
fn npx_command() -> Command {
    let mut cmd = crate::shell_env::command_with_login_path("npx");
    // The skills CLI is opt-out telemetry; never phone home from the hub.
    cmd.env("DISABLE_TELEMETRY", "1");
    cmd
}

/// skills.sh provider — installs via `npx skills add <owner/repo>`.
pub struct SkillsShProvider;

impl SkillProvider for SkillsShProvider {
    fn id(&self) -> &'static str {
        "skills.sh"
    }

    fn cli_check(&self) -> SkillCliStatus {
        // `npx skills add` is the documented entrypoint; npx fetches the CLI on
        // demand, so a working `npx` (Node) is the real prerequisite.
        match npx_command().arg("--version").output() {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                SkillCliStatus {
                    available: true,
                    version: (!version.is_empty()).then_some(version),
                    message: "npx is available — `skills` will be fetched on demand.".to_string(),
                }
            }
            Ok(_) | Err(_) => SkillCliStatus {
                available: false,
                version: None,
                message:
                    "npx (Node.js) was not found on PATH. Install Node 20+ to enable installs."
                        .to_string(),
            },
        }
    }

    fn search(&self, query: &str, limit: u32) -> Result<Vec<SkillSearchHit>> {
        if query.trim().len() < 2 {
            return Ok(Vec::new());
        }
        http_search(query, limit.clamp(1, 50))
    }

    fn install(
        &self,
        workspace_dir: &Path,
        install_ref: &str,
        skill: Option<&str>,
        tools: &[ToolId],
    ) -> Result<SkillInstallResult> {
        if !validate_install_ref(install_ref) {
            return Err(CoreError::InvalidSkillRef(install_ref.to_string()));
        }
        if let Some(slug) = skill {
            if !validate_skill_slug(slug) {
                return Err(CoreError::InvalidSkillRef(slug.to_string()));
            }
        }
        if !workspace_dir.is_dir() {
            return Err(CoreError::NotADirectory(workspace_dir.to_path_buf()));
        }
        let output = npx_command()
            .args(skills_npx_args(install_ref, skill, tools))
            .current_dir(workspace_dir)
            .output()
            .map_err(|e| match e.kind() {
                // The most common failure: `npx` isn't on the resolved PATH.
                // Turn the opaque "No such file or directory (os error 2)" into
                // an actionable hint.
                std::io::ErrorKind::NotFound => CoreError::SkillCliMissing,
                _ => CoreError::Io(e),
            })?;

        let mut log = String::new();
        let tool_list: Vec<&str> = tools.iter().map(|t| t.as_str()).collect();
        if !tool_list.is_empty() {
            log.push_str(&format!("target tools: {}\n", tool_list.join(", ")));
        }
        log.push_str(&String::from_utf8_lossy(&output.stdout));
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            log.push_str(&stderr);
        }
        Ok(SkillInstallResult {
            ok: output.status.success(),
            log: log.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_owner_repo_and_owner_repo_skill() {
        assert!(validate_install_ref("vercel-labs/agent-skills"));
        assert!(validate_install_ref("apollographql/skills"));
        assert!(validate_install_ref(
            "vercel-labs/agent-skills/next-js-development"
        ));
        assert!(validate_install_ref("a_b.c/d-e.f"));
    }

    #[test]
    fn rejects_unsafe_refs() {
        assert!(!validate_install_ref(""), "empty");
        assert!(!validate_install_ref("noslash"), "needs owner/repo");
        assert!(!validate_install_ref("/leading"), "empty component");
        assert!(!validate_install_ref("trailing/"), "empty component");
        assert!(!validate_install_ref("a//b"), "empty middle");
        assert!(!validate_install_ref("../etc/passwd"), "path traversal");
        assert!(!validate_install_ref("a/.."), "dotdot component");
        assert!(!validate_install_ref("a/b; rm -rf /"), "shell metachars");
        assert!(!validate_install_ref("a/b&&c"), "shell metachars");
        assert!(!validate_install_ref("a/b$(x)"), "shell metachars");
        assert!(!validate_install_ref("a b/c"), "space");
    }

    #[test]
    fn npx_args_pin_skill_and_agents_non_interactively() {
        // The slug and one --agent per tool keep `skills add` from opening its
        // interactive skill / agent pickers; the trailing --yes skips its
        // confirmation. Claude maps to the CLI's `claude-code`.
        assert_eq!(
            skills_npx_args(
                "vercel-labs/agent-skills",
                Some("vercel-react-best-practices"),
                &[ToolId::Cursor, ToolId::Claude],
            ),
            vec![
                "--yes".to_string(),
                "skills@latest".to_string(),
                "add".to_string(),
                "vercel-labs/agent-skills".to_string(),
                "--skill".to_string(),
                "vercel-react-best-practices".to_string(),
                "--agent".to_string(),
                "cursor".to_string(),
                "--agent".to_string(),
                "claude-code".to_string(),
                "--yes".to_string(),
            ]
        );
    }

    #[test]
    fn npx_args_omit_skill_and_agents_when_unspecified() {
        // Whole-repo, auto-detected agents: still wrapped by npx --yes + the
        // skills CLI --yes so a single-skill repo installs without a prompt.
        assert_eq!(
            skills_npx_args("vercel-labs/agent-skills", None, &[]),
            vec![
                "--yes".to_string(),
                "skills@latest".to_string(),
                "add".to_string(),
                "vercel-labs/agent-skills".to_string(),
                "--yes".to_string(),
            ]
        );
    }

    #[test]
    fn install_command_builds_the_pinned_npx_invocation() {
        let tools = [ToolId::Codex];
        let cmd = skills_install_command("vercel-labs/agent-skills", Some("pr-review"), &tools);
        assert_eq!(cmd.get_program().to_string_lossy(), "npx");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            skills_npx_args("vercel-labs/agent-skills", Some("pr-review"), &tools)
        );
    }

    #[test]
    fn update_args_pin_project_scope_non_interactively() {
        // `update <name> --project --yes` runs headless against the project lock.
        assert_eq!(
            skills_update_npx_args("rust-best-practices"),
            vec![
                "--yes".to_string(),
                "skills@latest".to_string(),
                "update".to_string(),
                "rust-best-practices".to_string(),
                "--project".to_string(),
                "--yes".to_string(),
            ]
        );
    }

    #[test]
    fn update_command_builds_the_pinned_npx_invocation() {
        let cmd = skills_update_command("rust-best-practices");
        assert_eq!(cmd.get_program().to_string_lossy(), "npx");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, skills_update_npx_args("rust-best-practices"));
    }

    #[test]
    fn validates_skill_slug() {
        assert!(validate_skill_slug("vercel-react-best-practices"));
        assert!(validate_skill_slug("pr_review.v2"));
        assert!(!validate_skill_slug(""), "empty");
        assert!(!validate_skill_slug("-rf"), "leads with a dash (flag-like)");
        assert!(!validate_skill_slug("a/b"), "slash");
        assert!(!validate_skill_slug("a b"), "space");
        assert!(!validate_skill_slug("a;b"), "metachar");
    }

    #[test]
    fn install_rejects_bad_ref_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let err = SkillsShProvider
            .install(dir.path(), "evil; rm -rf /", None, &[])
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidSkillRef(_)));
    }

    #[test]
    fn install_rejects_bad_slug_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let err = SkillsShProvider
            .install(dir.path(), "owner/repo", Some("--copy"), &[])
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidSkillRef(_)));
    }

    #[test]
    fn provider_lookup() {
        assert_eq!(provider_for("skills.sh").unwrap().id(), "skills.sh");
        assert!(provider_for("nope").is_none());
    }

    #[test]
    fn search_url_targets_keyless_endpoint_and_encodes_query() {
        assert_eq!(
            search_url("react native", 10),
            "https://skills.sh/api/search?q=react%20native&limit=10"
        );
        // Trims and never targets the key-gated /api/v1 surface.
        assert_eq!(
            search_url("  next  ", 5),
            "https://skills.sh/api/search?q=next&limit=5"
        );
    }

    #[test]
    fn parse_search_response_enriches_links() {
        let body = r#"{
            "query": "react",
            "searchType": "fuzzy",
            "skills": [
                {
                    "id": "vercel-labs/agent-skills/vercel-react-best-practices",
                    "skillId": "vercel-react-best-practices",
                    "name": "vercel-react-best-practices",
                    "installs": 448733,
                    "source": "vercel-labs/agent-skills"
                }
            ]
        }"#;
        let hits = parse_search_response(body).unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.skill_id, "vercel-react-best-practices");
        assert_eq!(hit.install_ref, "vercel-labs/agent-skills");
        assert_eq!(
            hit.github_url.as_deref(),
            Some("https://github.com/vercel-labs/agent-skills")
        );
        assert_eq!(
            hit.page_url,
            "https://skills.sh/vercel-labs/agent-skills/vercel-react-best-practices"
        );
        assert_eq!(hit.installs, 448733);
    }

    #[test]
    fn parse_search_response_drops_github_url_for_well_known_source() {
        let body = r#"{"skills":[{"id":"mintlify.com/mintlify","skillId":"mintlify","name":"Mintlify","installs":10,"source":"mintlify.com"}]}"#;
        let hits = parse_search_response(body).unwrap();
        assert!(hits[0].github_url.is_none());
        assert_eq!(hits[0].page_url, "https://skills.sh/mintlify.com/mintlify");
    }

    #[test]
    fn parse_search_response_rejects_malformed_json() {
        let err = parse_search_response("not json").unwrap_err();
        assert!(matches!(err, CoreError::SkillSearch(_)));
    }

    #[test]
    fn search_skips_network_for_short_queries() {
        // One char: returns empty without a request (no network in tests).
        assert!(SkillsShProvider.search("a", 10).unwrap().is_empty());
        assert!(SkillsShProvider.search("  ", 10).unwrap().is_empty());
    }
}
