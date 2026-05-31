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
    pub target_path: PathBuf,
    pub source_path: Option<PathBuf>,
    pub kind: OperationKind,
    pub reason: String,
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
