//! Pure domain crate for Agentic Hub.
//!
//! Owns the projection engine over the shared agentic root: scan, settings, and
//! (incrementally) adapter resolution, planning, applying, and sync. No Tauri
//! dependency — every public function is testable against a tempdir.
//!
//! See `ARCHITECTURE.projection.md` for the engine design and
//! `docs/tech/reference/shared-root-contract.md` for the filesystem contract.

pub mod adapter_registry;
pub mod agent_spec;
pub mod agent_version;
pub mod api;
pub mod applier;
pub mod bundle;
mod bundle_layout;
pub mod cli;
pub mod cli_tools;
pub mod client_telemetry;
pub mod codex_agent;
pub mod copilot_hook_sync;
pub mod error;
pub mod grok_hook_sync;
pub mod hook_sync;
pub mod internal_hooks;
pub mod kiro_hook_sync;
pub mod managed_copy;
pub mod mcp;
pub mod model;
pub mod open_targets;
pub mod paths;
pub mod planner;
pub mod reconcile;
pub mod rule_sync;
pub mod scaffold;
pub mod scanner;
pub mod sessions;
pub mod settings;
mod settings_shortcuts;
pub mod shell_env;
pub mod skill_favorites;
pub mod skill_lock;
pub mod skill_source;
pub mod source_skill_lock;
pub mod suite_binding_recovery;
pub mod suite_binding_store;
pub mod suite_store;
pub mod usage_store;
pub mod workspace_inventory;
pub mod workspace_target_store;

pub use adapter_registry::{
    create_workspace_adapter, Layout, ProjectionMode, ResolvedAdapter, WORKSPACE_TOOL_IDS,
};
pub use agent_spec::AgentSpec;
pub use api::{AdapterStatus, InspectResult};
pub use cli_tools::{
    bundled_catalog, check_tool, merge_catalogs, AuthState, CliTool, CliToolStatus,
};
pub use client_telemetry::{
    should_record_daily_active, utc_date_yyyy_mm_dd, ClientTelemetryState, ClientTelemetryStore,
};
pub use error::{CoreError, Result};
pub use hook_sync::{HookEventSpec, HookManifest};
pub use internal_hooks::{
    is_internal_item, usage_tracer_enabled, usage_tracer_hook_dir, usage_tracer_hook_id,
    usage_tracer_item, usage_tracer_manifest, usage_tracer_root, USAGE_TRACER_SCRIPT,
    USAGE_TRACER_TOOLS,
};
pub use model::{
    ApplyError, ApplyResult, ApplySuiteResult, CapabilityItem, CapabilityKind, ContentTransform,
    HookSyncError, HookSyncOutcome, LinkState, OperationKind, PlannedOperation, RuleSyncError,
    RuleSyncOutcome, ScanError, ScanResult, SourceRef, SuiteBinding, SuiteCapabilityRef,
    SuiteDefinition, SuiteValidationResult, SyncHooksResult, SyncRulesResult, ToolCapabilityState,
    ToolId, UsageStats, UsageToolBucket, WorkspaceTarget, WorkspaceTargetsState,
};
pub use open_targets::is_openable;
pub use planner::{build_plan, inspect_tool};
pub use reconcile::{reconcile_all, reconcile_tool, ReconcileToolOutcome};
pub use scaffold::{scaffold_demo, ScaffoldMode, ScaffoldResult};
pub use scanner::{scan, scan_all};
pub use sessions::{
    filter_sessions, list_all_sessions, read_session_transcript, SessionListFilter, SessionMessage,
    SessionRole, SessionSummary,
};
pub use settings::{
    ColorScheme, PaletteLaunchMode, PaletteQuickSearchShortcuts, Settings, SkillsConfig,
    SourceConfig, ToolSettings, ToolsSettings, UsageTracingConfig,
};
pub use skill_favorites::{SkillFavorite, SkillFavoritesState, SkillFavoritesStore};
pub use skill_lock::{parse_local_lock, read_local_lock, LocalSkillLock, LockedSkillEntry};
pub use skill_source::{
    provider_for, SkillCliStatus, SkillInstallResult, SkillProvider, SkillSearchHit,
    SkillsShProvider,
};
pub use suite_binding_recovery::resync_suite_binding;
pub use suite_binding_store::SuiteBindingStore;
pub use suite_store::{SuiteCreateInput, SuiteStore, SuiteUpdateInput};
pub use usage_store::{
    default_path as usage_store_path, hash_usage_correlation, UnresolvedUsageReference,
    UsageCapabilityQuery, UsageEventInput, UsageStore, UsageToolDiagnosticSummary,
};
pub use workspace_inventory::{
    scan_installed_tools, scan_workspace, InstalledToolInventory, LockedSkill, WorkspaceInventory,
};
pub use workspace_target_store::WorkspaceTargetStore;
