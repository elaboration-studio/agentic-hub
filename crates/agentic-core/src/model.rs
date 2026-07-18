use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The five capability kinds the scanner walks under each source root.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityKind {
    Skill,
    Agent,
    Rule,
    Hook,
    Command,
}

impl CapabilityKind {
    /// Every kind, in display / scan order.
    pub const ALL: [CapabilityKind; 5] = [
        CapabilityKind::Skill,
        CapabilityKind::Agent,
        CapabilityKind::Rule,
        CapabilityKind::Hook,
        CapabilityKind::Command,
    ];

    /// Top-level directory under a source root that holds this kind.
    pub fn dir_name(self) -> &'static str {
        match self {
            CapabilityKind::Skill => "skills",
            CapabilityKind::Agent => "agents",
            CapabilityKind::Rule => "rules",
            CapabilityKind::Hook => "hooks",
            CapabilityKind::Command => "commands",
        }
    }

    /// Prefix used in stable capability IDs (`skill:dev/tdd`, `hook:auto-format`).
    pub fn id_prefix(self) -> &'static str {
        match self {
            CapabilityKind::Skill => "skill",
            CapabilityKind::Agent => "agent",
            CapabilityKind::Rule => "rule",
            CapabilityKind::Hook => "hook",
            CapabilityKind::Command => "command",
        }
    }

    /// For directory-marker kinds, the file whose presence marks a capability.
    /// File-based kinds (agent, rule, command) return `None`.
    pub fn marker_file(self) -> Option<&'static str> {
        match self {
            CapabilityKind::Skill => Some("SKILL.md"),
            CapabilityKind::Hook => Some("hook.json"),
            CapabilityKind::Agent | CapabilityKind::Rule | CapabilityKind::Command => None,
        }
    }

    /// Allowed file extensions for file-based kinds (lowercase, no dot).
    pub fn file_extensions(self) -> &'static [&'static str] {
        match self {
            CapabilityKind::Agent => &["md"],
            CapabilityKind::Rule => &["md", "mdc"],
            CapabilityKind::Command => &["md"],
            CapabilityKind::Skill | CapabilityKind::Hook => &[],
        }
    }
}

/// A portable, cross-device identity for a capability source. Absolute paths
/// differ across machines, so we trace a source by its home-relative path
/// (primary) and its folder name (fallback). Two devices resolve "the same"
/// logical source by matching `rel_home`, then `folder`. See
/// `docs/tech/modules/multi-source-roots.md`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Home-relative path (e.g. `~/.agentic`); the absolute path for sources
    /// outside the home directory.
    pub rel_home: String,
    /// Last path component (e.g. `.agentic`).
    pub folder: String,
}

impl SourceRef {
    /// True when `self` and `other` denote the same logical source: equal
    /// `rel_home` (primary) or, failing that, equal `folder` (fallback for the
    /// same source mounted at a different home-relative path across devices).
    pub fn matches(&self, other: &SourceRef) -> bool {
        self.rel_home == other.rel_home || self.folder == other.folder
    }
}

/// A single scanned capability. IDs are source-free so suites resolve against
/// the whole source forest (first-source-wins). See `multi-source-roots.md`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    /// e.g. `skill:dev/repo-research`, `agent:coding/coding-agent.md`.
    pub id: String,
    pub kind: CapabilityKind,
    /// Display name: folder name for skill/hook, file stem for agent/rule.
    pub name: String,
    /// Absolute path. For hooks this is the hook folder (== `${HOOK_DIR}`).
    pub source_path: PathBuf,
    /// Path relative to `<source>/<kind-dir>/`.
    pub relative_path: PathBuf,
    /// Which source contributed this item.
    pub source_id: String,
    pub source_label: String,
    /// Portable cross-device identity of the contributing source.
    pub source: SourceRef,
    pub valid: bool,
    pub validation_errors: Vec<String>,
}

/// A non-fatal problem encountered during a scan (missing source, shadowing,
/// unreadable subdir). Surfaced to the UI; never aborts the scan.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

/// Result of scanning one or more source roots.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub items: Vec<CapabilityItem>,
    pub errors: Vec<ScanError>,
}

/// The eight supported AI tools.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolId {
    Codex,
    Claude,
    Cursor,
    Openclaw,
    /// The open-standard `~/.agents` directory shared across tools.
    Openstandard,
    /// Amazon Kiro IDE/CLI (`~/.kiro/`).
    Kiro,
    /// GitHub Copilot CLI (`~/.copilot/`).
    Copilot,
    /// Google Antigravity IDE/CLI (`~/.gemini/` shared layout).
    Antigravity,
}

impl ToolId {
    pub const ALL: [ToolId; 8] = [
        ToolId::Codex,
        ToolId::Claude,
        ToolId::Cursor,
        ToolId::Openclaw,
        ToolId::Openstandard,
        ToolId::Kiro,
        ToolId::Copilot,
        ToolId::Antigravity,
    ];

    /// Lowercase id, matching the serde wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolId::Codex => "codex",
            ToolId::Claude => "claude",
            ToolId::Cursor => "cursor",
            ToolId::Openclaw => "openclaw",
            ToolId::Openstandard => "openstandard",
            ToolId::Kiro => "kiro",
            ToolId::Copilot => "copilot",
            ToolId::Antigravity => "antigravity",
        }
    }
}

/// Per-(tool, item) projection state on disk.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Symlink to the correct source, or managed copy with matching hash, or
    /// rule present in the managed block.
    Enabled,
    /// Target absent.
    Disabled,
    /// Symlink to a non-existent path.
    Broken,
    /// Managed copy whose content drifted from the shared source.
    Stale,
    /// Real file/dir at the target, not owned by the manager.
    ForeignFile,
    /// Symlink/managed copy attributed to a different source.
    ForeignLink,
}

/// The inspected state of one capability for one tool.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCapabilityState {
    pub tool: ToolId,
    pub item_id: String,
    pub target_path: PathBuf,
    pub state: LinkState,
    pub current_link_target: Option<PathBuf>,
}

/// The mechanism a planned operation performs on disk.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    CreateLink,
    RemoveLink,
    ReplaceLink,
    CreateManagedCopy,
    RemoveManagedCopy,
    ReplaceManagedCopy,
    SyncJsonSection,
    ClearJsonSection,
    SkipConflict,
}

/// Content transform applied when writing a managed copy. `CodexAgentToml`
/// renders the markdown source as a Codex subagent TOML before writing (Codex
/// reads only `*.toml` in `~/.codex/agents/`); absence means the source bytes
/// are copied verbatim (every other managed copy).
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentTransform {
    CodexAgentToml,
}

/// A single planned mutation for one `(tool, item)`, computed by the planner
/// against fresh disk state.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOperation {
    pub tool: ToolId,
    pub item_id: String,
    /// The tool's base directory for this item's kind (skills/agents/rules dir).
    /// Carried so the applier can locate the per-root managed-copy manifest.
    pub target_root: PathBuf,
    pub target_path: PathBuf,
    pub source_path: Option<PathBuf>,
    pub kind: OperationKind,
    pub reason: String,
    /// User-authorized destructive take-over: the applier may delete a real
    /// (non-managed) file/dir at the target before projecting. Set only for a
    /// confirmed `foreign_file` resolution; always `false` for normal ops.
    #[serde(default)]
    pub force: bool,
    /// How the source maps onto the written bytes for managed-copy ops. `None`
    /// (the default) copies verbatim; `CodexAgentToml` renders the markdown
    /// source as Codex subagent TOML. Ignored by link and remove ops.
    #[serde(default)]
    pub content_transform: Option<ContentTransform>,
}

/// A per-operation failure. One failing op never aborts the rest.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyError {
    pub operation: PlannedOperation,
    pub message: String,
    pub code: String,
}

/// Aggregated outcome of applying a batch of operations.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub created: u32,
    pub removed: u32,
    pub replaced: u32,
    pub refreshed: u32,
    pub skipped: u32,
    pub errors: Vec<ApplyError>,
}

/// Outcome of a markdown managed-block rule sync for one tool.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSyncOutcome {
    Wrote,
    Removed,
    NoOp,
}

/// A rule-sync failure surfaced to the UI.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSyncError {
    pub path: PathBuf,
    pub code: String,
    pub message: String,
}

/// Result of `cmd_sync_rules`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRulesResult {
    pub outcome: RuleSyncOutcome,
    pub errors: Vec<RuleSyncError>,
}

/// Outcome of a hook `json_section` sync for one tool.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSyncOutcome {
    Wrote,
    Removed,
    NoOp,
}

/// A hook-sync failure surfaced to the UI.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSyncError {
    pub path: PathBuf,
    pub code: String,
    pub message: String,
}

/// Result of `cmd_sync_hooks`. `notes` carries non-fatal per-(event, tool)
/// "not supported; skipped" messages.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHooksResult {
    pub outcome: HookSyncOutcome,
    pub notes: Vec<String>,
    pub errors: Vec<HookSyncError>,
}

/// A named, tool-agnostic capability preset. Persisted in `~/.agentic-suites.json`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Source-qualified capability references. The bare `cap` matches
    /// `CapabilityItem.id`; the optional `source` makes the reference portable
    /// across devices. Legacy bare-string entries deserialize as `source:
    /// None`.
    pub capabilities: Vec<SuiteCapabilityRef>,
    /// When `true`, this suite's capabilities are unioned into every global
    /// suite apply, so its rules/skills are always present. At most one suite
    /// is base at a time (enforced by the store). Legacy files load as `false`.
    #[serde(default)]
    pub is_base: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A source-qualified suite entry. `cap` is a bare capability id
/// (`skill:dev/tdd`); `source` ties it to the contributing source so a synced
/// suite resolves per-source instead of mis-resolving onto a same-named
/// capability from a different source. Deserializes from a legacy bare string
/// (`"skill:dev/tdd"` -> `{ cap, source: None }`); always serializes as an
/// object so suite files upgrade in place on the next save.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteCapabilityRef {
    pub cap: String,
    pub source: Option<SourceRef>,
}

impl SuiteCapabilityRef {
    /// A reference with no source (legacy / unqualified).
    pub fn bare(cap: impl Into<String>) -> Self {
        Self {
            cap: cap.into(),
            source: None,
        }
    }

    /// True when this reference resolves to `item`: the bare cap id must match,
    /// and a qualified ref's source must match the item's source (home-relative
    /// path, then folder). An unqualified ref matches by id alone.
    pub fn matches_item(&self, item: &CapabilityItem) -> bool {
        self.cap == item.id
            && self
                .source
                .as_ref()
                .map_or(true, |s| s.matches(&item.source))
    }
}

impl From<&str> for SuiteCapabilityRef {
    fn from(cap: &str) -> Self {
        Self::bare(cap)
    }
}

impl From<String> for SuiteCapabilityRef {
    fn from(cap: String) -> Self {
        Self::bare(cap)
    }
}

impl<'de> Deserialize<'de> for SuiteCapabilityRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bare(String),
            Full {
                cap: String,
                #[serde(default)]
                source: Option<SourceRef>,
            },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Bare(cap) => SuiteCapabilityRef { cap, source: None },
            Raw::Full { cap, source } => SuiteCapabilityRef { cap, source },
        })
    }
}

/// Partition of a suite's capability IDs against a scan.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteValidationResult {
    pub valid_ids: Vec<String>,
    pub stale_ids: Vec<String>,
    /// Bare cap ids whose qualifying source is not present on this machine.
    /// They are preserved (never deleted), just not applicable here.
    pub absent_ids: Vec<String>,
}

/// Result of applying a suite to one tool (full reset through the pipeline).
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySuiteResult {
    pub apply_result: ApplyResult,
    /// Suite caps whose source is present (or unqualified) but no scanned item
    /// matches — genuinely stale references.
    pub skipped_stale: u32,
    /// Suite caps whose qualifying source is absent on this machine. Preserved,
    /// never deleted; just not applicable to a cross-device clone.
    pub skipped_absent_source: u32,
    pub suite: SuiteDefinition,
    /// Item ids enabled beyond the effective suite after this apply (persisted
    /// on the tool binding for the next switch / re-sync).
    pub manual_item_ids: Vec<String>,
}

/// The suite currently applied to one tool in global scope. Persisted so a
/// suite-capability edit can re-apply (full reset) to every bound tool. One
/// binding per tool — a full-reset apply makes a tool reflect exactly one
/// suite. See `docs/tech/modules/suite-bindings.md`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteBinding {
    pub tool_id: ToolId,
    pub suite_id: String,
    /// Capabilities the user enabled beyond the bound suite's effective set.
    #[serde(default)]
    pub manual_item_ids: Vec<String>,
}

/// One `(tool, item)` projection that a suite currently manages, surfaced so the
/// Manager matrix can lock the cell and name its owning suite on hover. Computed
/// from the live bindings, the suites, and the base suite against a fresh scan.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteOwnership {
    pub tool: ToolId,
    pub item_id: String,
    pub suite_id: String,
    pub suite_name: String,
    /// True when the owning suite is the base suite (merged in globally) rather
    /// than the tool's explicitly bound suite.
    pub from_base: bool,
}

/// Per-agentic-tool usage bucket for one capability.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageToolBucket {
    pub source_tool: String,
    pub execution_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    #[serde(default)]
    pub last_used_at: Option<String>,
}

/// Aggregated local usage stats for one scanned capability.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    pub capability_id: String,
    pub execution_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    #[serde(default)]
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub tool_buckets: Vec<UsageToolBucket>,
}

/// Date window for usage dashboard queries.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageDateRange {
    /// Rolling 24 hours (not calendar-day), i.e. `range_days` = 1.
    Today,
    Last7Days,
    Last30Days,
    Last90Days,
    AllTime,
}

/// High-level counters for the Statistics overview card row.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardOverview {
    pub total_events: u32,
    pub terminal_events: u32,
    pub resolved_events: u32,
    pub unresolved_events: u32,
    pub traced_capabilities: u32,
    pub installed_countable: u32,
    pub unused_countable: u32,
}

/// Terminal usage grouped by capability kind prefix.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageKindBucket {
    pub kind: String,
    pub execution_count: u32,
}

/// Terminal usage grouped by emitting agentic tool.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSourceBucket {
    pub source_tool: String,
    pub execution_count: u32,
}

/// Daily terminal usage for time-series charts.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDayBucket {
    pub day: String,
    pub execution_count: u32,
}

/// One heavily used capability joined with scan metadata.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTopRow {
    pub capability_id: String,
    pub name: String,
    pub kind: CapabilityKind,
    pub source_label: String,
    pub relative_path: String,
    pub execution_count: u32,
    #[serde(default)]
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub tool_buckets: Vec<UsageToolBucket>,
}

/// Installed skill/command/agent with zero attributed terminal usage.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageUnusedRow {
    pub capability_id: String,
    pub name: String,
    pub kind: CapabilityKind,
    pub source_label: String,
    pub relative_path: String,
}

/// Terminal usage grouped by workspace path.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWorkspaceBucket {
    pub workspace: String,
    pub execution_count: u32,
}

/// Aggregated local usage dashboard payload for the Statistics page.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboard {
    pub overview: UsageDashboardOverview,
    #[serde(default)]
    pub by_kind: Vec<UsageKindBucket>,
    #[serde(default)]
    pub by_source_tool: Vec<UsageSourceBucket>,
    #[serde(default)]
    pub by_day: Vec<UsageDayBucket>,
    #[serde(default)]
    pub top_capabilities: Vec<UsageTopRow>,
    #[serde(default)]
    pub today_top_capabilities: Vec<UsageTopRow>,
    #[serde(default)]
    pub unused_capabilities: Vec<UsageUnusedRow>,
    #[serde(default)]
    pub by_workspace: Vec<UsageWorkspaceBucket>,
}

/// A remembered per-project workspace directory (workspace scope).
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTarget {
    pub id: String,
    pub label: String,
    pub dir: PathBuf,
    pub last_used_at: String,
}

/// Persisted workspace-target state (`~/.agentic-hub/state.json`).
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTargetsState {
    #[serde(default)]
    pub workspace_targets: Vec<WorkspaceTarget>,
    #[serde(default)]
    pub workspace_active_id: Option<String>,
}
