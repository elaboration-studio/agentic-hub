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
pub mod cli_tools;
pub mod codex_agent;
pub mod copilot_hook_sync;
pub mod error;
pub mod hook_sync;
pub mod kiro_hook_sync;
pub mod managed_copy;
pub mod model;
pub mod open_targets;
pub mod paths;
pub mod planner;
pub mod reconcile;
pub mod rule_sync;
pub mod scaffold;
pub mod scanner;
pub mod settings;
pub mod shell_env;
pub mod skill_favorites;
pub mod skill_lock;
pub mod skill_source;
pub mod suite_binding_store;
pub mod suite_store;
pub mod workspace_inventory;
pub mod workspace_target_store;

pub use adapter_registry::{
    create_workspace_adapter, Layout, ProjectionMode, ResolvedAdapter, WORKSPACE_TOOL_IDS,
};
pub use api::{AdapterStatus, InspectResult};
pub use cli_tools::{
    bundled_catalog, check_tool, merge_catalogs, AuthState, CliTool, CliToolStatus,
};
pub use error::{CoreError, Result};
pub use hook_sync::{HookEventSpec, HookManifest};
pub use model::{
    ApplyError, ApplyResult, ApplySuiteResult, CapabilityItem, CapabilityKind, ContentTransform,
    HookSyncError, HookSyncOutcome, LinkState, OperationKind, PlannedOperation, RuleSyncError,
    RuleSyncOutcome, ScanError, ScanResult, SourceRef, SuiteBinding, SuiteCapabilityRef,
    SuiteDefinition, SuiteValidationResult, SyncHooksResult, SyncRulesResult, ToolCapabilityState,
    ToolId, WorkspaceTarget, WorkspaceTargetsState,
};
pub use open_targets::is_openable;
pub use planner::{build_plan, inspect_tool};
pub use reconcile::{reconcile_all, reconcile_tool, ReconcileToolOutcome};
pub use scaffold::{scaffold_demo, ScaffoldMode, ScaffoldResult};
pub use scanner::{scan, scan_all};
pub use settings::{Settings, SkillsConfig, SourceConfig, ToolSettings, ToolsSettings};
pub use skill_favorites::{SkillFavorite, SkillFavoritesState, SkillFavoritesStore};
pub use skill_lock::{parse_local_lock, read_local_lock, LocalSkillLock, LockedSkillEntry};
pub use skill_source::{
    provider_for, SkillCliStatus, SkillInstallResult, SkillProvider, SkillSearchHit,
    SkillsShProvider,
};
pub use suite_binding_store::SuiteBindingStore;
pub use suite_store::{SuiteCreateInput, SuiteStore, SuiteUpdateInput};
pub use workspace_inventory::{
    scan_installed_tools, scan_workspace, InstalledToolInventory, LockedSkill, WorkspaceInventory,
};
pub use workspace_target_store::WorkspaceTargetStore;
