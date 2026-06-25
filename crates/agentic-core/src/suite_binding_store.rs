//! Persistence for global suite<->tool bindings (`~/.agentic-hub/suite-bindings.json`).
//! Records which suite is currently applied to each tool so a suite-capability
//! edit can re-apply (full reset) to every bound tool. Atomic writes; one
//! binding per tool (a full-reset apply makes a tool reflect exactly one suite).
//!
//! See `docs/tech/modules/suite-bindings.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{SuiteBinding, ToolId};
use crate::paths::home_dir;

/// On-disk envelope. `version` allows future migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BindingFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    bindings: Vec<SuiteBinding>,
}

fn default_version() -> u32 {
    1
}

impl Default for BindingFile {
    fn default() -> Self {
        BindingFile {
            version: 1,
            bindings: Vec::new(),
        }
    }
}

/// Dotfile-backed suite-binding store.
pub struct SuiteBindingStore {
    path: PathBuf,
}

impl Default for SuiteBindingStore {
    fn default() -> Self {
        SuiteBindingStore {
            path: default_path(),
        }
    }
}

/// Canonical state path: `~/.agentic-hub/suite-bindings.json`.
pub fn default_path() -> PathBuf {
    home_dir().join(".agentic-hub").join("suite-bindings.json")
}

impl SuiteBindingStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct against an explicit path (tests).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        SuiteBindingStore { path: path.into() }
    }

    fn read_file(&self) -> Result<BindingFile> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|e| CoreError::StateParse(e.to_string()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(BindingFile::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    fn write_file(&self, file: &BindingFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(file)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// All current bindings.
    pub fn read(&self) -> Result<Vec<SuiteBinding>> {
        Ok(self.read_file()?.bindings)
    }

    /// Bind a suite to a tool. Upserts by tool — a tool can only reflect one
    /// suite at a time (full-reset apply), so a fresh apply replaces the prior.
    pub fn record(&self, tool: ToolId, suite_id: &str, preserve_unmanaged: bool) -> Result<()> {
        let mut file = self.read_file()?;
        file.bindings.retain(|b| b.tool_id != tool);
        file.bindings.push(SuiteBinding {
            tool_id: tool,
            suite_id: suite_id.to_string(),
            preserve_unmanaged,
        });
        self.write_file(&file)
    }

    /// Tools currently bound to this suite, in `ToolId::ALL` order.
    pub fn tools_for_suite(&self, suite_id: &str) -> Result<Vec<ToolId>> {
        let file = self.read_file()?;
        Ok(ToolId::ALL
            .into_iter()
            .filter(|tool| {
                file.bindings
                    .iter()
                    .any(|b| b.tool_id == *tool && b.suite_id == suite_id)
            })
            .collect())
    }

    /// Drop every binding referencing this suite (e.g. on suite delete). The
    /// tools' on-disk projections are left untouched.
    pub fn drop_suite(&self, suite_id: &str) -> Result<()> {
        let mut file = self.read_file()?;
        file.bindings.retain(|b| b.suite_id != suite_id);
        self.write_file(&file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SuiteBindingStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SuiteBindingStore::with_path(dir.path().join("suite-bindings.json"));
        (dir, store)
    }

    #[test]
    fn missing_file_reads_empty() {
        let (_d, store) = store();
        assert!(store.read().unwrap().is_empty());
        assert!(store.tools_for_suite("any").unwrap().is_empty());
    }

    #[test]
    fn record_upserts_per_tool() {
        let (_d, store) = store();
        store.record(ToolId::Codex, "suite-a", false).unwrap();
        store.record(ToolId::Claude, "suite-b", false).unwrap();
        // Re-recording the same tool replaces, never duplicates.
        store.record(ToolId::Codex, "suite-c", true).unwrap();

        let bindings = store.read().unwrap();
        assert_eq!(bindings.len(), 2);
        let codex = bindings
            .iter()
            .find(|b| b.tool_id == ToolId::Codex)
            .unwrap();
        assert_eq!(codex.suite_id, "suite-c");
        assert!(codex.preserve_unmanaged);
    }

    #[test]
    fn record_round_trips_preserve_unmanaged() {
        let (_d, store) = store();
        store.record(ToolId::Codex, "suite-a", true).unwrap();
        let bindings = store.read().unwrap();
        assert!(bindings[0].preserve_unmanaged);
        store.record(ToolId::Codex, "suite-a", false).unwrap();
        let bindings = store.read().unwrap();
        assert!(!bindings[0].preserve_unmanaged);
    }

    #[test]
    fn legacy_binding_without_preserve_deserializes_as_false() {
        let (_d, store) = store();
        let json = r#"{"version":1,"bindings":[{"toolId":"codex","suiteId":"suite-a"}]}"#;
        fs::write(&store.path, json).unwrap();
        let bindings = store.read().unwrap();
        assert_eq!(bindings.len(), 1);
        assert!(!bindings[0].preserve_unmanaged);
    }

    #[test]
    fn tools_for_suite_filters_and_orders() {
        let (_d, store) = store();
        store.record(ToolId::Cursor, "suite-a", false).unwrap();
        store.record(ToolId::Codex, "suite-a", false).unwrap();
        store.record(ToolId::Claude, "suite-b", false).unwrap();

        // Returned in ToolId::ALL order (codex before cursor), not insert order.
        assert_eq!(
            store.tools_for_suite("suite-a").unwrap(),
            vec![ToolId::Codex, ToolId::Cursor]
        );
        assert_eq!(
            store.tools_for_suite("suite-b").unwrap(),
            vec![ToolId::Claude]
        );
        assert!(store.tools_for_suite("suite-gone").unwrap().is_empty());
    }

    #[test]
    fn drop_suite_removes_only_its_bindings() {
        let (_d, store) = store();
        store.record(ToolId::Codex, "suite-a", false).unwrap();
        store.record(ToolId::Claude, "suite-b", false).unwrap();

        store.drop_suite("suite-a").unwrap();

        let bindings = store.read().unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].suite_id, "suite-b");
    }

    #[test]
    fn malformed_file_is_error_not_overwrite() {
        let (_d, store) = store();
        fs::write(&store.path, "{ not json").unwrap();
        assert!(matches!(store.read(), Err(CoreError::StateParse(_))));
        assert_eq!(fs::read_to_string(&store.path).unwrap(), "{ not json");
    }
}
