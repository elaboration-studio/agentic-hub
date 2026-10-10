//! The headless `ehub` command: `version`, `agents list`, `agents show <id>`,
//! `bundle <id> --harness <h>`. Writes exactly one JSON document and returns
//! the contract exit code. Read-only outside `~/.agentic-hub/bundles/`. See
//! `docs/tech/reference/ehub-cli-contract.md`.

use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{json, Value};

use crate::agent_version::{agent_version, resolve_caps};
use crate::bundle::{self, BundleContext, BundleError, Harness};
use crate::cli_tools::{self, CliTool};
use crate::error::CoreError;
use crate::model::{CapabilityItem, CapabilityKind, SuiteDefinition};
use crate::scanner;
use crate::settings::Settings;
use crate::suite_store::SuiteStore;

const SCHEMA: u32 = 1;
const CONTRACT: u32 = 1;

/// Everything a command reads, injected so tests stay hermetic.
pub struct CliContext {
    pub settings: Settings,
    pub bundles_root: PathBuf,
    /// `true` when the CLI with this catalog id is installed.
    pub probe: Box<dyn Fn(&str) -> bool + Sync>,
    pub now: SystemTime,
}

impl CliContext {
    /// The files the app reads: `~/.agentic-hub/config.json` (and through it
    /// the suites file and source roots), the CLI catalog, and the bundles
    /// root. `HOME` decides where all of them live.
    pub fn from_env() -> Result<CliContext, CoreError> {
        let settings = Settings::load()?;
        let catalog = load_catalog(&settings)?;
        Ok(CliContext {
            settings,
            bundles_root: bundle::default_bundles_root(),
            probe: Box::new(move |id| {
                catalog
                    .iter()
                    .find(|t| t.id == id)
                    .is_some_and(cli_tools::is_installed)
            }),
            now: SystemTime::now(),
        })
    }
}

/// Bundled catalog merged with the user's override. A broken override falls
/// back to the bundled set (with a stderr note) instead of failing the run.
fn load_catalog(settings: &Settings) -> Result<Vec<CliTool>, CoreError> {
    let bundled = cli_tools::bundled_catalog()?;
    let Some(path) = settings.resolved_cli_tools_path() else {
        return Ok(bundled);
    };
    let custom = match std::fs::read_to_string(&path) {
        Ok(text) => cli_tools::parse_catalog(&text),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(bundled),
        Err(e) => Err(e.into()),
    };
    match custom {
        Ok(custom) => Ok(cli_tools::merge_catalogs(bundled, custom)),
        Err(e) => {
            eprintln!("ehub: ignoring CLI tools override: {e}");
            Ok(bundled)
        }
    }
}

/// Run one command. `args` are the words after `ehub` (no program name).
pub fn run(args: &[String], out: &mut impl Write) -> i32 {
    run_with(args, out, CliContext::from_env)
}

/// [`run`] with an injected context loader, called only after the arguments
/// are validated (so a bad id or harness never touches the filesystem).
pub fn run_with<F>(args: &[String], out: &mut impl Write, load: F) -> i32
where
    F: FnOnce() -> Result<CliContext, CoreError>,
{
    let result = parse(args).and_then(|command| match command {
        Command::Version => Ok(version_doc()),
        command => {
            let ctx = load().map_err(|e| CliError::Internal(e.to_string()))?;
            execute(command, &ctx)
        }
    });
    let (code, doc) = match result {
        Ok(doc) => (0, doc),
        Err(e) => (
            e.exit_code(),
            json!({ "schema": SCHEMA, "error": { "code": e.code(), "message": e.to_string() } }),
        ),
    };
    match writeln!(out, "{doc}") {
        Ok(()) => code,
        Err(_) => 1,
    }
}

fn version_doc() -> Value {
    json!({ "schema": SCHEMA, "contract": CONTRACT, "appVersion": env!("CARGO_PKG_VERSION") })
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Version,
    AgentsList,
    AgentsShow(String),
    Bundle(String, Harness),
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    InvalidArgument(String),
    #[error("No suite with id {0}")]
    NotFound(String),
    #[error("Unsupported harness {0:?} (expected cursor, claude, codex, or grok)")]
    UnsupportedHarness(String),
    #[error("{0}")]
    Internal(String),
}

impl CliError {
    fn code(&self) -> &'static str {
        match self {
            CliError::InvalidArgument(_) => "INVALID_ARGUMENT",
            CliError::NotFound(_) => "NOT_FOUND",
            CliError::UnsupportedHarness(_) => "UNSUPPORTED_HARNESS",
            CliError::Internal(_) => "INTERNAL",
        }
    }

    fn exit_code(&self) -> i32 {
        match self {
            CliError::Internal(_) => 1,
            CliError::InvalidArgument(_) => 2,
            CliError::NotFound(_) => 3,
            CliError::UnsupportedHarness(_) => 4,
        }
    }
}

fn invalid(message: impl Into<String>) -> CliError {
    CliError::InvalidArgument(message.into())
}

/// Pure: validates every id and harness before anything is read.
fn parse(args: &[String]) -> Result<Command, CliError> {
    let mut words: Vec<&str> = Vec::new();
    let mut harness: Option<&str> = None;
    let mut iter = args.iter().map(String::as_str);
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            continue;
        }
        if arg == "--harness" {
            harness = Some(
                iter.next()
                    .ok_or_else(|| invalid("--harness needs a value"))?,
            );
        } else if let Some(value) = arg.strip_prefix("--harness=") {
            harness = Some(value);
        } else if arg.starts_with('-') {
            return Err(invalid(format!("Unknown flag {arg}")));
        } else {
            words.push(arg);
        }
    }
    let command = match words.as_slice() {
        ["version"] => Command::Version,
        ["agents", "list"] => Command::AgentsList,
        ["agents", "show", id] => Command::AgentsShow(valid_id(id)?),
        ["bundle", id] => {
            let id = valid_id(id)?;
            let name = harness
                .take()
                .ok_or_else(|| invalid("bundle needs --harness <cursor|claude|codex|grok>"))?;
            let harness =
                Harness::parse(name).ok_or_else(|| CliError::UnsupportedHarness(name.into()))?;
            Command::Bundle(id, harness)
        }
        [] => return Err(invalid("Missing command")),
        _ => return Err(invalid(format!("Unknown command: {}", words.join(" ")))),
    };
    if harness.is_some() {
        return Err(invalid("--harness is only valid for bundle"));
    }
    Ok(command)
}

fn valid_id(id: &str) -> Result<String, CliError> {
    if bundle::is_valid_suite_id(id) {
        Ok(id.to_string())
    } else {
        Err(invalid(format!("Invalid suite id {id:?}")))
    }
}

fn execute(command: Command, ctx: &CliContext) -> Result<Value, CliError> {
    let suites = SuiteStore::with_path(ctx.settings.resolved_suites_path())
        .list()
        .map_err(|e| CliError::Internal(e.to_string()))?;
    let items = scanner::scan_all(&ctx.settings.resolve_sources()).items;
    match command {
        Command::Version => Ok(version_doc()),
        Command::AgentsList => {
            let agents: Vec<AgentSummary<'_>> = suites
                .iter()
                .map(|s| AgentSummary::new(s, &items))
                .collect();
            Ok(json!({ "schema": SCHEMA, "agents": agents }))
        }
        Command::AgentsShow(id) => {
            let suite = find(&suites, &id)?;
            let detail = AgentDetail {
                summary: AgentSummary::new(suite, &items),
                capabilities: resolve_caps(suite, &items)
                    .into_iter()
                    .map(|c| c.id)
                    .collect(),
            };
            Ok(json!({ "schema": SCHEMA, "agent": detail }))
        }
        Command::Bundle(id, harness) => {
            let bctx = BundleContext {
                bundles_root: ctx.bundles_root.clone(),
                suites: &suites,
                items: &items,
                probe: &*ctx.probe,
                now: ctx.now,
                gc_max_age: bundle::GC_MAX_AGE,
            };
            let manifest = bundle::build_bundle(&bctx, &id, harness).map_err(|e| match e {
                BundleError::InvalidId(_) => invalid(e.to_string()),
                BundleError::NotFound(id) => CliError::NotFound(id),
                BundleError::Io(_) | BundleError::Json(_) => CliError::Internal(e.to_string()),
            })?;
            Ok(json!({ "schema": SCHEMA, "bundle": manifest }))
        }
    }
}

fn find<'a>(suites: &'a [SuiteDefinition], id: &str) -> Result<&'a SuiteDefinition, CliError> {
    suites
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| CliError::NotFound(id.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSummary<'a> {
    id: &'a str,
    name: &'a str,
    emoji: Option<&'a str>,
    description: Option<&'a str>,
    version: String,
    is_agent: bool,
    capability_counts: CapabilityCounts,
    required_clis: &'a [String],
}

impl<'a> AgentSummary<'a> {
    fn new(suite: &'a SuiteDefinition, items: &[CapabilityItem]) -> Self {
        let agent = suite.agent.as_ref();
        AgentSummary {
            id: &suite.id,
            name: &suite.name,
            emoji: agent.and_then(|a| a.emoji.as_deref()),
            description: suite.description.as_deref(),
            version: agent_version(suite, items),
            is_agent: agent.is_some(),
            capability_counts: CapabilityCounts::of(suite, items),
            required_clis: agent.map_or(&[], |a| a.required_clis.as_slice()),
        }
    }
}

#[derive(Serialize)]
struct AgentDetail<'a> {
    #[serde(flatten)]
    summary: AgentSummary<'a>,
    capabilities: Vec<String>,
}

/// Counts the suite's own refs by id prefix, found or not, so they agree
/// with `agents show`'s `capabilities`.
#[derive(Serialize, Default)]
struct CapabilityCounts {
    skill: u32,
    agent: u32,
    rule: u32,
    hook: u32,
    command: u32,
    mcp: u32,
}

impl CapabilityCounts {
    fn of(suite: &SuiteDefinition, items: &[CapabilityItem]) -> Self {
        let mut counts = CapabilityCounts::default();
        for cap in resolve_caps(suite, items) {
            let prefix = cap.id.split(':').next().unwrap_or_default();
            let slot = match CapabilityKind::ALL
                .into_iter()
                .find(|k| k.id_prefix() == prefix)
            {
                Some(CapabilityKind::Skill) => &mut counts.skill,
                Some(CapabilityKind::Agent) => &mut counts.agent,
                Some(CapabilityKind::Rule) => &mut counts.rule,
                Some(CapabilityKind::Hook) => &mut counts.hook,
                Some(CapabilityKind::Command) => &mut counts.command,
                Some(CapabilityKind::Mcp) => &mut counts.mcp,
                None => continue,
            };
            *slot += 1;
        }
        counts
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
