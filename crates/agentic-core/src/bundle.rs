//! Immutable per-run agent bundles under `~/.agentic-hub/bundles/<suiteId>/
//! <version>/<harness>/`. A bundle appears through one atomic rename, so
//! readers never see a half-written one and concurrent builders of the same
//! version both succeed without a lock. See `docs/tech/modules/agent-bundles.md`
//! and the wire contract in `docs/tech/reference/ehub-cli-contract.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::agent_version::{agent_version, resolve_caps};
use crate::bundle_layout::{self, LayoutInput};
use crate::mcp::McpTransport;
use crate::model::{CapabilityItem, SuiteDefinition, ToolId};

/// Sibling versions untouched for this long are deleted after a build.
pub const GC_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub const MANIFEST_FILE: &str = "bundle.json";

const MAX_SUITE_ID_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Cursor,
    Claude,
    Codex,
    Grok,
}

impl Harness {
    pub const ALL: [Harness; 4] = [
        Harness::Cursor,
        Harness::Claude,
        Harness::Codex,
        Harness::Grok,
    ];

    pub fn parse(name: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Harness::Cursor => "cursor",
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Grok => "grok",
        }
    }

    pub fn mount(self) -> Mount {
        match self {
            Harness::Cursor | Harness::Claude => Mount::Plugin,
            Harness::Codex | Harness::Grok => Mount::Prompt,
        }
    }

    pub(crate) fn tool(self) -> ToolId {
        match self {
            Harness::Cursor => ToolId::Cursor,
            Harness::Claude => ToolId::Claude,
            Harness::Codex => ToolId::Codex,
            Harness::Grok => ToolId::Grok,
        }
    }
}

/// How the caller hands the bundle to the CLI: a `--plugin-dir`, or folded
/// into the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mount {
    Plugin,
    Prompt,
}

/// `bundle.json`, and the `bundle` object of `ehub bundle`. Every path is
/// absolute and inside `root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub agent_id: String,
    pub name: String,
    pub version: String,
    pub harness: Harness,
    pub mount: Mount,
    pub root: PathBuf,
    pub plugin_dir: Option<PathBuf>,
    pub instructions_file: Option<PathBuf>,
    pub skills: Vec<BundleSkill>,
    pub mcp_servers: Vec<BundleMcpServer>,
    pub required_clis: Vec<RequiredCli>,
    pub skipped: Vec<SkippedCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleSkill {
    pub name: String,
    pub path: PathBuf,
}

/// Carries env var names, never values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleMcpServer {
    pub name: String,
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredCli {
    pub id: String,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedCapability {
    pub capability: String,
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("invalid suite id: {0:?}")]
    InvalidId(String),
    #[error("No suite with id {0}")]
    NotFound(String),
    #[error("bundle write failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("bundle manifest is malformed: {0}")]
    Json(#[from] serde_json::Error),
}

/// Everything a build reads, injected so tests stay hermetic.
pub struct BundleContext<'a> {
    /// `~/.agentic-hub/bundles` in production ([`default_bundles_root`]).
    pub bundles_root: PathBuf,
    pub suites: &'a [SuiteDefinition],
    pub items: &'a [CapabilityItem],
    /// `true` when the CLI with this catalog id is installed.
    pub probe: &'a (dyn Fn(&str) -> bool + Sync),
    pub now: SystemTime,
    pub gc_max_age: Duration,
}

pub fn default_bundles_root() -> PathBuf {
    crate::paths::home_dir()
        .join(".agentic-hub")
        .join("bundles")
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$` — safe as one path component.
pub fn is_valid_suite_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.len() <= MAX_SUITE_ID_CHARS
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Build (or reuse) the bundle for the suite's current version and `harness`.
/// The suite id is validated before any filesystem access; the base suite's
/// capabilities are never merged in.
pub fn build_bundle(
    ctx: &BundleContext<'_>,
    suite_id: &str,
    harness: Harness,
) -> Result<BundleManifest, BundleError> {
    if !is_valid_suite_id(suite_id) {
        return Err(BundleError::InvalidId(suite_id.to_string()));
    }
    let suite = ctx
        .suites
        .iter()
        .find(|s| s.id == suite_id)
        .ok_or_else(|| BundleError::NotFound(suite_id.to_string()))?;
    let version = agent_version(suite, ctx.items);
    let version_dir = ctx.bundles_root.join(suite_id).join(&version);
    let root = version_dir.join(harness.as_str());

    if let Some(mut existing) = read_manifest(&root)? {
        touch(&version_dir, ctx.now);
        existing.required_clis = bundle_layout::required_clis(suite, ctx.probe);
        return Ok(existing);
    }

    fs::create_dir_all(&version_dir)?;
    let stage = version_dir.join(stage_name(harness));
    let caps = resolve_caps(suite, ctx.items);
    let input = LayoutInput {
        stage: &stage,
        root: &root,
        suite,
        version: &version,
        harness,
        caps: &caps,
        probe: ctx.probe,
    };
    let manifest = match stage_bundle(&input) {
        Ok(m) => m,
        Err(e) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(e);
        }
    };

    match publish(&stage, &root)? {
        Some(winner) => Ok(winner),
        None => {
            collect_garbage(&ctx.bundles_root.join(suite_id), &version, ctx);
            Ok(manifest)
        }
    }
}

/// Rename `stage` into place. `Ok(None)` when this call published; `Ok(Some)`
/// with the winner's manifest when another builder got there first.
fn publish(stage: &Path, root: &Path) -> Result<Option<BundleManifest>, BundleError> {
    match fs::rename(stage, root) {
        Ok(()) => Ok(None),
        Err(e) => {
            let _ = fs::remove_dir_all(stage);
            match read_manifest(root)? {
                Some(winner) => Ok(Some(winner)),
                None => Err(BundleError::Io(e)),
            }
        }
    }
}

fn stage_bundle(input: &LayoutInput<'_>) -> Result<BundleManifest, BundleError> {
    fs::create_dir(input.stage)?;
    let manifest = bundle_layout::write_layout(input)?;
    let body = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    fs::write(input.stage.join(MANIFEST_FILE), body)?;
    Ok(manifest)
}

fn read_manifest(root: &Path) -> Result<Option<BundleManifest>, BundleError> {
    match fs::read_to_string(root.join(MANIFEST_FILE)) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Unique per process and per call, so threads of one process never share a
/// staging dir.
fn stage_name(harness: Harness) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{}.tmp-{}-{nanos}-{}",
        harness.as_str(),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Best effort: a failed touch only makes the version eligible for GC sooner.
fn touch(dir: &Path, now: SystemTime) {
    let _ = fs::File::open(dir).and_then(|f| f.set_modified(now));
}

/// Delete sibling versions idle longer than `gc_max_age`. Never the current
/// one; never anything that is not a real directory. Best effort.
fn collect_garbage(suite_dir: &Path, current: &str, ctx: &BundleContext<'_>) {
    let Ok(entries) = fs::read_dir(suite_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() == current || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| ctx.now.duration_since(modified).ok())
            .is_some_and(|age| age > ctx.gc_max_age);
        if stale {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
#[path = "bundle_test_fixture.rs"]
mod test_fixture;

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bundle_lifecycle_tests.rs"]
mod lifecycle_tests;
