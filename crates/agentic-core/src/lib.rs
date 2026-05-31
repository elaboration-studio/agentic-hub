//! Pure domain crate for Agentic Hub.
//!
//! Owns the projection engine over the shared agentic root: scan, settings, and
//! (incrementally) adapter resolution, planning, applying, and sync. No Tauri
//! dependency — every public function is testable against a tempdir.
//!
//! See `ARCHITECTURE.projection.md` for the engine design and
//! `docs/tech/reference/shared-root-contract.md` for the filesystem contract.

pub mod adapter_registry;
pub mod api;
pub mod applier;
pub mod error;
pub mod hook_sync;
pub mod managed_copy;
pub mod model;
pub mod paths;
pub mod planner;
pub mod reconcile;
pub mod rule_sync;
pub mod scanner;
pub mod settings;
pub mod suite_store;
pub mod workspace_patch;
pub mod workspace_target_store;

pub use adapter_registry::{
    create_workspace_adapter, Layout, ProjectionMode, ResolvedAdapter, WORKSPACE_TOOL_IDS,
};
pub use api::{AdapterStatus, InspectResult};
pub use error::{CoreError, Result};
pub use hook_sync::{HookEventSpec, HookManifest};
pub use model::{
    ApplyError, ApplyResult, ApplySuiteResult, CapabilityItem, CapabilityKind, HookSyncError,
    HookSyncOutcome, LinkState, OperationKind, PlannedOperation, RuleSyncError, RuleSyncOutcome,
    ScanError, ScanResult, SuiteDefinition, SuiteValidationResult, SyncHooksResult,
    SyncRulesResult, ToolCapabilityState, ToolId, WorkspaceApply, WorkspacePatchResult,
    WorkspaceTarget, WorkspaceTargetsState,
};
pub use planner::{build_plan, inspect_tool};
pub use reconcile::{reconcile_all, reconcile_tool, ReconcileToolOutcome};
pub use scanner::{scan, scan_all};
pub use settings::{Settings, SourceConfig, ToolSettings, ToolsSettings};
pub use suite_store::{SuiteCreateInput, SuiteStore, SuiteUpdateInput};
pub use workspace_target_store::WorkspaceTargetStore;
