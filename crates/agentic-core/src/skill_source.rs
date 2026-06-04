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
    /// Install `install_ref` into `workspace_dir`. `tools` is the user's target
    /// selection (advisory; the CLI auto-detects agents from the project).
    fn install(
        &self,
        workspace_dir: &Path,
        install_ref: &str,
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

/// The fixed `npx` argument vector for installing a (pre-validated) ref. Kept
/// pure so the exact invocation is asserted in tests. `--yes` makes npx
/// non-interactive; `skills@latest` pins to the published CLI.
pub fn skills_npx_args(install_ref: &str) -> Vec<String> {
    vec![
        "--yes".to_string(),
        "skills@latest".to_string(),
        "add".to_string(),
        install_ref.to_string(),
    ]
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

/// Best-effort `PATH` from the user's login shell. GUI apps launched from the
/// macOS Dock inherit a minimal `PATH` that omits Homebrew / nvm, so `npx` is
/// often invisible. We read the login shell's `PATH` once and hand it to the
/// child. `None` falls back to the inherited environment.
fn login_path() -> Option<String> {
    for shell in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
        if !Path::new(shell).exists() {
            continue;
        }
        if let Ok(out) = Command::new(shell).args(["-lc", "printf %s \"$PATH\""]).output() {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Build an `npx` command with the login `PATH` applied and telemetry disabled.
fn npx_command() -> Command {
    let mut cmd = Command::new("npx");
    if let Some(path) = login_path() {
        cmd.env("PATH", path);
    }
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
                message: "npx (Node.js) was not found on PATH. Install Node 20+ to enable installs."
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
        tools: &[ToolId],
    ) -> Result<SkillInstallResult> {
        if !validate_install_ref(install_ref) {
            return Err(CoreError::InvalidSkillRef(install_ref.to_string()));
        }
        if !workspace_dir.is_dir() {
            return Err(CoreError::NotADirectory(workspace_dir.to_path_buf()));
        }
        let output = npx_command()
            .args(skills_npx_args(install_ref))
            .current_dir(workspace_dir)
            .output()
            .map_err(CoreError::Io)?;

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
        assert!(validate_install_ref("vercel-labs/agent-skills/next-js-development"));
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
    fn npx_args_are_fixed_and_nonshell() {
        assert_eq!(
            skills_npx_args("vercel-labs/agent-skills"),
            vec![
                "--yes".to_string(),
                "skills@latest".to_string(),
                "add".to_string(),
                "vercel-labs/agent-skills".to_string(),
            ]
        );
    }

    #[test]
    fn install_rejects_bad_ref_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let err = SkillsShProvider
            .install(dir.path(), "evil; rm -rf /", &[])
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
