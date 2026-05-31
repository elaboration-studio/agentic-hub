//! Persistence for remembered workspace directories (`~/.agentic-hub/state.json`).
//! Upserts by canonical path, bumps `lastUsedAt`, sets the touched entry active,
//! and enforces a 12-entry LRU cap. See `docs/tech/modules/workspace-patch.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::managed_copy::now_iso8601;
use crate::model::{ToolId, WorkspaceApply, WorkspaceTarget, WorkspaceTargetsState};
use crate::paths::home_dir;

const LRU_CAP: usize = 12;

/// Dotfile-backed workspace-target store.
pub struct WorkspaceTargetStore {
    path: PathBuf,
}

impl Default for WorkspaceTargetStore {
    fn default() -> Self {
        WorkspaceTargetStore {
            path: default_path(),
        }
    }
}

/// Canonical state path: `~/.agentic-hub/state.json`.
pub fn default_path() -> PathBuf {
    home_dir().join(".agentic-hub").join("state.json")
}

impl WorkspaceTargetStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        WorkspaceTargetStore { path: path.into() }
    }

    pub fn read(&self) -> Result<WorkspaceTargetsState> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|e| CoreError::StateParse(e.to_string()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(WorkspaceTargetsState::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    fn write(&self, state: &WorkspaceTargetsState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(state)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Upsert a directory: merge by canonical path, bump `lastUsedAt`, set active,
    /// enforce the LRU cap. Returns the active target.
    pub fn add(&self, dir: &Path) -> Result<WorkspaceTarget> {
        if !dir.is_dir() {
            return Err(CoreError::NotADirectory(dir.to_path_buf()));
        }
        let canonical = dir.canonicalize()?;
        let label = canonical
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| canonical.to_string_lossy().to_string());
        let now = now_iso8601();

        let mut state = self.read()?;
        let id = match state
            .workspace_targets
            .iter_mut()
            .find(|t| t.dir == canonical)
        {
            Some(existing) => {
                existing.last_used_at = now.clone();
                existing.label = label;
                existing.id.clone()
            }
            None => {
                let id = format!("ws-{}", uuid::Uuid::new_v4());
                state.workspace_targets.push(WorkspaceTarget {
                    id: id.clone(),
                    label,
                    dir: canonical.clone(),
                    last_used_at: now.clone(),
                    last_applied: Vec::new(),
                });
                id
            }
        };

        // LRU: pin the just-touched entry to the front (it always survives the
        // cap, regardless of timestamp resolution), then keep the most recent rest.
        let active = state
            .workspace_targets
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .expect("just-added target is present");
        let mut rest: Vec<WorkspaceTarget> = state
            .workspace_targets
            .into_iter()
            .filter(|t| t.id != id)
            .collect();
        rest.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
        rest.truncate(LRU_CAP - 1);
        let mut targets = Vec::with_capacity(rest.len() + 1);
        targets.push(active.clone());
        targets.extend(rest);
        state.workspace_targets = targets;
        state.workspace_active_id = Some(id);

        self.write(&state)?;
        Ok(active)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut state = self.read()?;
        state.workspace_targets.retain(|t| t.id != id);
        if state.workspace_active_id.as_deref() == Some(id) {
            state.workspace_active_id = state.workspace_targets.first().map(|t| t.id.clone());
        }
        self.write(&state)
    }

    /// Record the suite last applied to a workspace for one tool (upsert by
    /// tool). No-op if the workspace id is unknown. The watcher replays these.
    pub fn record_apply(&self, id: &str, tool: ToolId, suite_id: &str) -> Result<()> {
        let mut state = self.read()?;
        if let Some(t) = state.workspace_targets.iter_mut().find(|t| t.id == id) {
            t.last_applied.retain(|a| a.tool_id != tool);
            t.last_applied.push(WorkspaceApply {
                tool_id: tool,
                suite_id: suite_id.to_string(),
            });
        }
        self.write(&state)
    }

    pub fn set_active(&self, id: &str) -> Result<()> {
        let mut state = self.read()?;
        if !state.workspace_targets.iter().any(|t| t.id == id) {
            return Err(CoreError::PathNotFound(PathBuf::from(id)));
        }
        state.workspace_active_id = Some(id.to_string());
        self.write(&state)
    }

    pub fn get_active(&self) -> Result<Option<WorkspaceTarget>> {
        let state = self.read()?;
        Ok(state
            .workspace_active_id
            .and_then(|id| state.workspace_targets.into_iter().find(|t| t.id == id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, WorkspaceTargetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceTargetStore::with_path(dir.path().join("state.json"));
        (dir, store)
    }

    #[test]
    fn add_upserts_by_canonical_path_and_sets_active() {
        let (_d, store) = store();
        let ws = tempfile::tempdir().unwrap();
        let first = store.add(ws.path()).unwrap();
        let second = store.add(ws.path()).unwrap();
        assert_eq!(first.id, second.id, "same dir merges to one entry");

        let state = store.read().unwrap();
        assert_eq!(state.workspace_targets.len(), 1);
        assert_eq!(
            state.workspace_active_id.as_deref(),
            Some(first.id.as_str())
        );
    }

    #[test]
    fn remove_clears_active_when_needed() {
        let (_d, store) = store();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ta = store.add(a.path()).unwrap();
        let tb = store.add(b.path()).unwrap();
        assert_eq!(store.get_active().unwrap().unwrap().id, tb.id);

        store.remove(&tb.id).unwrap();
        // Active falls back to a remaining entry.
        assert_eq!(store.get_active().unwrap().unwrap().id, ta.id);
        store.remove(&ta.id).unwrap();
        assert!(store.get_active().unwrap().is_none());
    }

    #[test]
    fn set_active_rejects_unknown() {
        let (_d, store) = store();
        assert!(store.set_active("nope").is_err());
    }

    #[test]
    fn record_apply_upserts_per_tool() {
        let (_d, store) = store();
        let ws = tempfile::tempdir().unwrap();
        let t = store.add(ws.path()).unwrap();

        store.record_apply(&t.id, ToolId::Codex, "suite-a").unwrap();
        store.record_apply(&t.id, ToolId::Claude, "suite-b").unwrap();
        // Re-applying the same tool replaces, never duplicates.
        store.record_apply(&t.id, ToolId::Codex, "suite-c").unwrap();

        let stored = store
            .read()
            .unwrap()
            .workspace_targets
            .into_iter()
            .find(|x| x.id == t.id)
            .unwrap();
        assert_eq!(stored.last_applied.len(), 2);
        let codex = stored
            .last_applied
            .iter()
            .find(|a| a.tool_id == ToolId::Codex)
            .unwrap();
        assert_eq!(codex.suite_id, "suite-c");
    }

    #[test]
    fn lru_cap_keeps_twelve_most_recent() {
        let (_d, store) = store();
        let dirs: Vec<_> = (0..14).map(|_| tempfile::tempdir().unwrap()).collect();
        let mut last_id = String::new();
        for d in &dirs {
            last_id = store.add(d.path()).unwrap().id;
        }
        let state = store.read().unwrap();
        assert_eq!(state.workspace_targets.len(), LRU_CAP);
        // The most-recently-added entry is always retained and active.
        assert_eq!(state.workspace_active_id.as_deref(), Some(last_id.as_str()));
        assert!(state.workspace_targets.iter().any(|t| t.id == last_id));
    }
}
