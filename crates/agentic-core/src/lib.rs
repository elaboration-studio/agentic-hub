//! Pure domain crate for Agentic Hub.
//!
//! Owns the projection engine over the shared agentic root: scan, settings, and
//! (incrementally) adapter resolution, planning, applying, and sync. No Tauri
//! dependency — every public function is testable against a tempdir.
//!
//! See `ARCHITECTURE.projection.md` for the engine design and
//! `docs/tech/reference/shared-root-contract.md` for the filesystem contract.

pub mod error;
pub mod model;
pub mod paths;
pub mod scanner;
pub mod settings;

pub use error::{CoreError, Result};
pub use model::{CapabilityItem, CapabilityKind, ScanError, ScanResult, ToolId};
pub use scanner::{scan, scan_all};
pub use settings::{Settings, SourceConfig, ToolSettings, ToolsSettings};
