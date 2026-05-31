use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The four capability kinds the scanner walks under each source root.
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
}

impl CapabilityKind {
    /// Every kind, in display / scan order.
    pub const ALL: [CapabilityKind; 4] = [
        CapabilityKind::Skill,
        CapabilityKind::Agent,
        CapabilityKind::Rule,
        CapabilityKind::Hook,
    ];

    /// Top-level directory under a source root that holds this kind.
    pub fn dir_name(self) -> &'static str {
        match self {
            CapabilityKind::Skill => "skills",
            CapabilityKind::Agent => "agents",
            CapabilityKind::Rule => "rules",
            CapabilityKind::Hook => "hooks",
        }
    }

    /// Prefix used in stable capability IDs (`skill:dev/tdd`, `hook:auto-format`).
    pub fn id_prefix(self) -> &'static str {
        match self {
            CapabilityKind::Skill => "skill",
            CapabilityKind::Agent => "agent",
            CapabilityKind::Rule => "rule",
            CapabilityKind::Hook => "hook",
        }
    }

    /// For directory-marker kinds, the file whose presence marks a capability.
    /// File-based kinds (agent, rule) return `None`.
    pub fn marker_file(self) -> Option<&'static str> {
        match self {
            CapabilityKind::Skill => Some("SKILL.md"),
            CapabilityKind::Hook => Some("hook.json"),
            CapabilityKind::Agent | CapabilityKind::Rule => None,
        }
    }

    /// Allowed file extensions for file-based kinds (lowercase, no dot).
    pub fn file_extensions(self) -> &'static [&'static str] {
        match self {
            CapabilityKind::Agent => &["md"],
            CapabilityKind::Rule => &["md", "mdc"],
            CapabilityKind::Skill | CapabilityKind::Hook => &[],
        }
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

/// The four supported AI tools.
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
}

impl ToolId {
    pub const ALL: [ToolId; 4] = [
        ToolId::Codex,
        ToolId::Claude,
        ToolId::Cursor,
        ToolId::Openclaw,
    ];
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
    /// Source-free capability IDs matching `CapabilityItem.id`.
    pub capabilities: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
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
    /// Capability IDs in the suite not provided by any configured source.
    pub skipped_stale: u32,
    pub suite: SuiteDefinition,
}

/// The suite last applied to a workspace for one tool. Lets the watcher
/// re-patch a workspace's hard copies from fresh source content on change.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceApply {
    pub tool_id: ToolId,
    pub suite_id: String,
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
    /// Per-tool last-applied suite, recorded on each workspace patch. The
    /// watcher replays these to keep the workspace in sync. Defaults to empty
    /// for entries written before this field existed.
    #[serde(default)]
    pub last_applied: Vec<WorkspaceApply>,
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

/// Outcome of applying a suite into a workspace directory (hard copy).
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePatchResult {
    pub tool: ToolId,
    pub workspace_dir: PathBuf,
    pub suite_id: String,
    pub suite_name: String,
    /// Workspace-relative paths written this cycle (incl. managed sentinels).
    pub applied: Vec<String>,
    /// Prior-manifest entries cleaned this cycle.
    pub removed: Vec<String>,
    /// Suite capability IDs not provided by any configured source.
    pub skipped_stale_ids: Vec<String>,
    pub notes: Vec<String>,
    pub errors: Vec<String>,
}
