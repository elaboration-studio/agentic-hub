//! CRUD over the suite dotfile `~/.agentic-suites.json` (path preserved from the
//! VS Code extension for migration parity). Atomic writes, name-uniqueness, and
//! validation against a scan. No projection logic lives here.
//!
//! See `docs/tech/modules/suite-presets.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::managed_copy::now_iso8601;
use crate::model::{CapabilityItem, SuiteDefinition, SuiteValidationResult};
use crate::paths::home_dir;

/// On-disk envelope. `version` allows future migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SuiteFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    suites: Vec<SuiteDefinition>,
}

fn default_version() -> u32 {
    1
}

impl Default for SuiteFile {
    fn default() -> Self {
        SuiteFile {
            version: 1,
            suites: Vec::new(),
        }
    }
}

/// Fields accepted when creating a suite.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteCreateInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Partial update; `None` fields are left unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteUpdateInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
}

/// Dotfile-backed suite store.
pub struct SuiteStore {
    path: PathBuf,
}

impl Default for SuiteStore {
    fn default() -> Self {
        SuiteStore {
            path: default_path(),
        }
    }
}

/// Canonical dotfile path: `~/.agentic-suites.json`.
pub fn default_path() -> PathBuf {
    home_dir().join(".agentic-suites.json")
}

impl SuiteStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct against an explicit path (tests).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        SuiteStore { path: path.into() }
    }

    fn read_file(&self) -> Result<SuiteFile> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|e| CoreError::SuiteParse(e.to_string()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(SuiteFile::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    fn write_file(&self, file: &SuiteFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(file)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<SuiteDefinition>> {
        Ok(self.read_file()?.suites)
    }

    pub fn get(&self, id: &str) -> Result<Option<SuiteDefinition>> {
        Ok(self.read_file()?.suites.into_iter().find(|s| s.id == id))
    }

    pub fn create(&self, input: SuiteCreateInput) -> Result<SuiteDefinition> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(CoreError::SuiteNameConflict("(empty name)".to_string()));
        }
        let mut file = self.read_file()?;
        if file.suites.iter().any(|s| s.name == name) {
            return Err(CoreError::SuiteNameConflict(name));
        }
        let now = now_iso8601();
        let suite = SuiteDefinition {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: input.description,
            capabilities: input.capabilities,
            created_at: now.clone(),
            updated_at: now,
        };
        file.suites.push(suite.clone());
        self.write_file(&file)?;
        Ok(suite)
    }

    pub fn update(&self, id: &str, input: SuiteUpdateInput) -> Result<SuiteDefinition> {
        let mut file = self.read_file()?;
        // Reject a rename that collides with a different suite.
        if let Some(new_name) = input.name.as_ref().map(|n| n.trim().to_string()) {
            if file.suites.iter().any(|s| s.name == new_name && s.id != id) {
                return Err(CoreError::SuiteNameConflict(new_name));
            }
        }
        let pos = file
            .suites
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| CoreError::SuiteNotFound(id.to_string()))?;
        let suite = &mut file.suites[pos];
        if let Some(name) = input.name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(CoreError::SuiteNameConflict("(empty name)".to_string()));
            }
            suite.name = name;
        }
        if let Some(description) = input.description {
            suite.description = description;
        }
        if let Some(capabilities) = input.capabilities {
            suite.capabilities = capabilities;
        }
        suite.updated_at = now_iso8601();
        let updated = suite.clone();
        self.write_file(&file)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut file = self.read_file()?;
        let before = file.suites.len();
        file.suites.retain(|s| s.id != id);
        if file.suites.len() == before {
            return Err(CoreError::SuiteNotFound(id.to_string()));
        }
        self.write_file(&file)
    }

    /// Partition a suite's capability IDs into those present in the scan and
    /// those missing from every configured source.
    pub fn validate(suite: &SuiteDefinition, items: &[CapabilityItem]) -> SuiteValidationResult {
        let mut valid_ids = Vec::new();
        let mut stale_ids = Vec::new();
        for cap in &suite.capabilities {
            if items.iter().any(|i| &i.id == cap) {
                valid_ids.push(cap.clone());
            } else {
                stale_ids.push(cap.clone());
            }
        }
        SuiteValidationResult {
            valid_ids,
            stale_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CapabilityKind;
    use std::path::PathBuf;

    fn store() -> (tempfile::TempDir, SuiteStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SuiteStore::with_path(dir.path().join(".agentic-suites.json"));
        (dir, store)
    }

    fn item(id: &str) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind: CapabilityKind::Skill,
            name: id.to_string(),
            source_path: PathBuf::from("/src"),
            relative_path: PathBuf::from(id),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            valid: true,
            validation_errors: vec![],
        }
    }

    fn create(name: &str, caps: &[&str]) -> SuiteCreateInput {
        SuiteCreateInput {
            name: name.to_string(),
            description: None,
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn missing_file_lists_empty() {
        let (_d, store) = store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn create_get_update_remove_roundtrip() {
        let (_d, store) = store();
        let made = store.create(create("coding", &["skill:dev/tdd"])).unwrap();
        assert_eq!(made.name, "coding");
        assert!(!made.id.is_empty());

        let fetched = store.get(&made.id).unwrap().unwrap();
        assert_eq!(fetched.capabilities, vec!["skill:dev/tdd"]);

        let updated = store
            .update(
                &made.id,
                SuiteUpdateInput {
                    capabilities: Some(vec!["skill:a".into(), "rule:b".into()]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.capabilities.len(), 2);
        assert_eq!(updated.created_at, made.created_at, "created_at preserved");

        store.remove(&made.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(matches!(
            store.remove(&made.id),
            Err(CoreError::SuiteNotFound(_))
        ));
    }

    #[test]
    fn name_uniqueness_enforced() {
        let (_d, store) = store();
        store.create(create("dup", &[])).unwrap();
        assert!(matches!(
            store.create(create("dup", &[])),
            Err(CoreError::SuiteNameConflict(_))
        ));
        // Rename collision is also rejected.
        let other = store.create(create("other", &[])).unwrap();
        assert!(matches!(
            store.update(
                &other.id,
                SuiteUpdateInput {
                    name: Some("dup".into()),
                    ..Default::default()
                }
            ),
            Err(CoreError::SuiteNameConflict(_))
        ));
    }

    #[test]
    fn malformed_file_is_error_not_overwrite() {
        let (_d, store) = store();
        fs::write(&store.path, "{ not json").unwrap();
        assert!(matches!(store.list(), Err(CoreError::SuiteParse(_))));
        assert_eq!(fs::read_to_string(&store.path).unwrap(), "{ not json");
    }

    #[test]
    fn validate_partitions_stale() {
        let suite = SuiteDefinition {
            id: "x".into(),
            name: "s".into(),
            description: None,
            capabilities: vec!["skill:live".into(), "skill:gone".into()],
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let result = SuiteStore::validate(&suite, &[item("skill:live")]);
        assert_eq!(result.valid_ids, vec!["skill:live"]);
        assert_eq!(result.stale_ids, vec!["skill:gone"]);
    }
}
