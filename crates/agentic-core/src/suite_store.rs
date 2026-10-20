//! CRUD over the suite dotfile `~/.agentic-suites.json` (path preserved from the
//! VS Code extension for migration parity). Atomic writes, name-uniqueness, and
//! validation against a scan. No projection logic lives here.
//!
//! See `docs/tech/modules/suite-presets.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent_spec::AgentSpec;
use crate::error::{CoreError, Result};
use crate::internal_hooks;
use crate::managed_copy::now_iso8601;
use crate::model::{CapabilityItem, SuiteCapabilityRef, SuiteDefinition, SuiteValidationResult};
use crate::paths::home_dir;
use crate::settings::{source_present, SourceConfig};

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
    /// Source-qualified refs. Legacy bare-string entries deserialize fine via
    /// [`SuiteCapabilityRef`]'s tolerant impl.
    #[serde(default)]
    pub capabilities: Vec<SuiteCapabilityRef>,
    #[serde(default)]
    pub agent: Option<AgentSpec>,
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
    pub capabilities: Option<Vec<SuiteCapabilityRef>>,
    /// Mark/unmark this suite as the single base suite. `Some(true)` clears the
    /// base flag on every other suite; `Some(false)` just unsets this one.
    #[serde(default)]
    pub is_base: Option<bool>,
    /// `Some(None)` (JSON `null`) clears the agent block; absent leaves it.
    #[serde(default, deserialize_with = "present_or_null")]
    pub agent: Option<Option<AgentSpec>>,
}

/// Distinguish a present `null` (`Some(None)`) from an absent field (`None`).
fn present_or_null<'de, D, T>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn validate_agent(agent: Option<&AgentSpec>) -> Result<()> {
    agent.map_or(Ok(()), |a| a.validate().map_err(CoreError::InvalidAgent))
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
        // Keep a one-level backup of the last good file before overwriting, so a
        // clobber (a git merge gone wrong on the synced suites file) leaves the
        // prior content at `<file>.bak` for recovery.
        crate::paths::back_up_dotfile(&self.path);
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
        validate_agent(input.agent.as_ref())?;
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
            is_base: false,
            agent: input.agent,
            created_at: now.clone(),
            updated_at: now,
        };
        file.suites.push(suite.clone());
        self.write_file(&file)?;
        Ok(suite)
    }

    pub fn update(&self, id: &str, input: SuiteUpdateInput) -> Result<SuiteDefinition> {
        validate_agent(input.agent.as_ref().and_then(Option::as_ref))?;
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
        if let Some(is_base) = input.is_base {
            suite.is_base = is_base;
        }
        if let Some(agent) = input.agent {
            suite.agent = agent;
        }
        suite.updated_at = now_iso8601();
        let updated = suite.clone();
        // Single-base invariant: marking this suite base clears every other.
        if input.is_base == Some(true) {
            for s in &mut file.suites {
                if s.id != id {
                    s.is_base = false;
                }
            }
        }
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

    /// Replace an existing suite in place — used to persist a source backfill /
    /// legacy upgrade. No timestamp bump and no name-uniqueness check: the suite
    /// already exists and its content is not a user edit. Errors on unknown id.
    pub fn put(&self, suite: &SuiteDefinition) -> Result<()> {
        let mut file = self.read_file()?;
        let pos = file
            .suites
            .iter()
            .position(|s| s.id == suite.id)
            .ok_or_else(|| CoreError::SuiteNotFound(suite.id.clone()))?;
        file.suites[pos] = suite.clone();
        self.write_file(&file)
    }

    /// The single base suite, if one is marked.
    pub fn base(&self) -> Result<Option<SuiteDefinition>> {
        Ok(self.read_file()?.suites.into_iter().find(|s| s.is_base))
    }

    /// Set the base suite to `id` (clearing every other), or clear all bases
    /// when `None`. Errors on an unknown id. Enforces the single-base invariant.
    pub fn set_base(&self, id: Option<&str>) -> Result<()> {
        let mut file = self.read_file()?;
        if let Some(id) = id {
            if !file.suites.iter().any(|s| s.id == id) {
                return Err(CoreError::SuiteNotFound(id.to_string()));
            }
            for s in &mut file.suites {
                s.is_base = s.id == id;
            }
        } else {
            for s in &mut file.suites {
                s.is_base = false;
            }
        }
        self.write_file(&file)
    }

    /// Partition a suite's capability refs against a scan and the local source
    /// list: `valid` resolve to a scanned item; `absent` are qualified to a
    /// source not present here (preserved, never deleted); `stale` are
    /// present-source (or unqualified) refs that match no scanned item.
    pub fn validate(
        suite: &SuiteDefinition,
        items: &[CapabilityItem],
        sources: &[SourceConfig],
    ) -> SuiteValidationResult {
        let mut valid_ids = Vec::new();
        let mut stale_ids = Vec::new();
        let mut absent_ids = Vec::new();
        for r in &suite.capabilities {
            if items
                .iter()
                .any(|i| !internal_hooks::is_internal_item(i) && r.matches_item(i))
            {
                valid_ids.push(r.cap.clone());
            } else if matches!(&r.source, Some(s) if !source_present(sources, s)) {
                absent_ids.push(r.cap.clone());
            } else {
                stale_ids.push(r.cap.clone());
            }
        }
        SuiteValidationResult {
            valid_ids,
            stale_ids,
            absent_ids,
        }
    }

    /// Qualify unqualified refs whose bare id resolves to exactly one scanned
    /// item, attaching that item's source. Already-qualified, ambiguous, or
    /// unresolved refs are left untouched. Returns `true` if anything changed so
    /// the caller can persist the upgrade. The scanner is first-source-wins, so
    /// in practice every resolvable bare id has a single match.
    pub fn backfill_sources(suite: &mut SuiteDefinition, items: &[CapabilityItem]) -> bool {
        let mut changed = false;
        for r in &mut suite.capabilities {
            if r.source.is_some() {
                continue;
            }
            let mut hits = items
                .iter()
                .filter(|i| !internal_hooks::is_internal_item(i) && i.id == r.cap);
            if let (Some(first), None) = (hits.next(), hits.next()) {
                r.source = Some(first.source.clone());
                changed = true;
            }
        }
        changed
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
        item_from(id, "~/.agentic", ".agentic")
    }

    fn item_from(id: &str, rel_home: &str, folder: &str) -> CapabilityItem {
        CapabilityItem {
            id: id.to_string(),
            kind: CapabilityKind::Skill,
            name: id.to_string(),
            source_path: PathBuf::from("/src"),
            relative_path: PathBuf::from(id),
            source_id: "arno".into(),
            source_label: "Arno".into(),
            source: crate::model::SourceRef {
                rel_home: rel_home.into(),
                folder: folder.into(),
            },
            valid: true,
            validation_errors: vec![],
        }
    }

    fn create(name: &str, caps: &[&str]) -> SuiteCreateInput {
        SuiteCreateInput {
            name: name.to_string(),
            description: None,
            capabilities: caps.iter().map(|s| (*s).into()).collect(),
            agent: None,
        }
    }

    fn source(label: &str, path: &str) -> SourceConfig {
        SourceConfig {
            id: String::new(),
            label: label.into(),
            path: PathBuf::from(path),
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
        let caps: Vec<&str> = fetched
            .capabilities
            .iter()
            .map(|r| r.cap.as_str())
            .collect();
        assert_eq!(caps, vec!["skill:dev/tdd"]);

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
            is_base: false,
            agent: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let sources = vec![source("Arno", "~/.agentic")];
        let result = SuiteStore::validate(&suite, &[item("skill:live")], &sources);
        assert_eq!(result.valid_ids, vec!["skill:live"]);
        assert_eq!(result.stale_ids, vec!["skill:gone"]);
        assert!(result.absent_ids.is_empty());
    }

    #[test]
    fn validate_marks_absent_source_separately_from_stale() {
        // A ref qualified to a source that is not configured locally.
        let absent = SuiteCapabilityRef {
            cap: "skill:remote".into(),
            source: Some(crate::model::SourceRef {
                rel_home: "~/other-machine".into(),
                folder: "other-machine".into(),
            }),
        };
        let suite = SuiteDefinition {
            id: "x".into(),
            name: "s".into(),
            description: None,
            capabilities: vec![absent, "skill:gone".into()],
            is_base: false,
            agent: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let sources = vec![source("Arno", "~/.agentic")];
        let result = SuiteStore::validate(&suite, &[item("skill:live")], &sources);
        assert_eq!(result.absent_ids, vec!["skill:remote"], "source not here");
        assert_eq!(result.stale_ids, vec!["skill:gone"], "present-source, gone");
        assert!(result.valid_ids.is_empty());
    }

    #[test]
    fn legacy_bare_string_file_loads_and_upgrades_to_objects() {
        let (_d, store) = store();
        // A suite file written before source-qualification: bare-string caps.
        fs::write(
            &store.path,
            r#"{ "version": 1, "suites": [
                { "id": "s1", "name": "coding", "description": null,
                  "capabilities": ["skill:dev/tdd", "rule:style"],
                  "createdAt": "t", "updatedAt": "t" }
            ] }"#,
        )
        .unwrap();

        // Tolerant deserialize reads the bare strings as unqualified refs.
        let loaded = store.get("s1").unwrap().unwrap();
        assert_eq!(loaded.capabilities.len(), 2);
        assert!(loaded.capabilities.iter().all(|r| r.source.is_none()));
        assert_eq!(loaded.capabilities[0].cap, "skill:dev/tdd");

        // Any write upgrades the on-disk shape to objects.
        store.put(&loaded).unwrap();
        let raw = fs::read_to_string(&store.path).unwrap();
        assert!(raw.contains("\"cap\""), "upgraded to object form: {raw}");
        assert!(raw.contains("\"source\""));
    }

    #[test]
    fn write_keeps_a_backup_of_the_prior_good_file() {
        let (_d, store) = store();
        store.create(create("first", &["skill:a"])).unwrap();
        let first = fs::read_to_string(&store.path).unwrap();

        // A later write backs the prior content up to `<file>.bak` for recovery.
        store.create(create("second", &["skill:b"])).unwrap();
        let mut bak = store.path.as_os_str().to_os_string();
        bak.push(".bak");
        assert_eq!(fs::read_to_string(PathBuf::from(bak)).unwrap(), first);
    }

    #[test]
    fn backfill_is_in_memory_only_and_never_rewrites_the_file() {
        // Regression guard for the cross-device git-sync corruption: applying or
        // editing a suite must never silently rewrite the synced suites file with
        // device-specific source qualifications (that divergence is what a later
        // `git pull` line-merges into empty capability arrays).
        let (_d, store) = store();
        let made = store.create(create("coding", &["skill:a"])).unwrap();
        let before = fs::read_to_string(&store.path).unwrap();

        // The apply/update path loads the suite and qualifies its refs in memory.
        let mut suite = store.get(&made.id).unwrap().unwrap();
        let items = vec![item_from("skill:a", "~/.agentic", ".agentic")];
        assert!(
            SuiteStore::backfill_sources(&mut suite, &items),
            "backfill reports a change in memory"
        );

        // ...but the on-disk file is byte-identical: no persist, no churn.
        assert_eq!(fs::read_to_string(&store.path).unwrap(), before);
    }

    #[test]
    fn backfill_qualifies_unqualified_refs_from_scan() {
        let mut suite = SuiteDefinition {
            id: "s".into(),
            name: "s".into(),
            description: None,
            capabilities: vec!["skill:a".into(), "skill:missing".into()],
            is_base: false,
            agent: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let items = vec![item_from("skill:a", "~/.agentic", ".agentic")];
        let changed = SuiteStore::backfill_sources(&mut suite, &items);
        assert!(changed);
        // skill:a now carries the scanned source; skill:missing stays bare.
        let a = suite
            .capabilities
            .iter()
            .find(|r| r.cap == "skill:a")
            .unwrap();
        assert_eq!(a.source.as_ref().unwrap().folder, ".agentic");
        let missing = suite
            .capabilities
            .iter()
            .find(|r| r.cap == "skill:missing")
            .unwrap();
        assert!(missing.source.is_none());
    }

    #[test]
    fn set_base_enforces_single_base() {
        let (_d, store) = store();
        let a = store.create(create("a", &[])).unwrap();
        let b = store.create(create("b", &[])).unwrap();
        assert!(store.base().unwrap().is_none(), "no base by default");

        store.set_base(Some(&a.id)).unwrap();
        assert_eq!(store.base().unwrap().unwrap().id, a.id);

        // Marking b base clears a.
        store.set_base(Some(&b.id)).unwrap();
        let base = store.base().unwrap().unwrap();
        assert_eq!(base.id, b.id);
        assert!(!store.get(&a.id).unwrap().unwrap().is_base, "a cleared");

        // Clearing leaves no base.
        store.set_base(None).unwrap();
        assert!(store.base().unwrap().is_none());

        assert!(matches!(
            store.set_base(Some("nope")),
            Err(CoreError::SuiteNotFound(_))
        ));
    }

    #[test]
    fn update_is_base_clears_other_bases() {
        let (_d, store) = store();
        let a = store.create(create("a", &[])).unwrap();
        let b = store.create(create("b", &[])).unwrap();
        store.set_base(Some(&a.id)).unwrap();

        // Updating b to base via the generic update path clears a.
        let updated = store
            .update(
                &b.id,
                SuiteUpdateInput {
                    is_base: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(updated.is_base);
        assert!(!store.get(&a.id).unwrap().unwrap().is_base, "a cleared");
        assert_eq!(store.base().unwrap().unwrap().id, b.id);
    }
}

#[cfg(test)]
#[path = "suite_store_agent_tests.rs"]
mod agent_tests;
