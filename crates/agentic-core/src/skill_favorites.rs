//! Local store of "starred" skills from public sources (skills.sh today).
//! Persisted at `~/.agentic-hub/skills-favorites.json` (override via settings).
//!
//! Favorites are a *local reference list* of skills the user likes — name,
//! stable id, source repo, and the links needed to jump to GitHub / skills.sh.
//! We do **not** sync starred state with any remote: skills.sh has no favorites
//! API. The list is reused across projects to install the same skills again.
//!
//! See `docs/tech/modules/skill-sources.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::managed_copy::now_iso8601;
use crate::paths::home_dir;

/// One starred skill. `provider` namespaces the id so two sources can share a
/// slug. `install_ref` is what a provider hands its installer (for skills.sh,
/// the `owner/repo` form the CLI accepts). Links are best-effort.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFavorite {
    /// Source provider id, e.g. `"skills.sh"`.
    pub provider: String,
    /// Stable, provider-scoped id (for skills.sh: `"{source}/{slug}"`).
    pub id: String,
    /// URL-safe skill slug, e.g. `"next-js-development"`.
    pub slug: String,
    /// Human-readable name.
    pub name: String,
    /// Origin repo / provider, e.g. `"vercel-labs/agent-skills"`.
    pub source: String,
    /// Reference handed to the provider's installer. For skills.sh this is the
    /// `owner/repo` (or `owner/repo/skill`) the CLI accepts.
    pub install_ref: String,
    /// GitHub (or well-known base) URL for the skill, if known.
    #[serde(default)]
    pub github_url: Option<String>,
    /// The skill's page URL on the provider site (skills.sh), if known.
    #[serde(default)]
    pub page_url: Option<String>,
    /// ISO-8601 timestamp this entry was starred; set by the store on add.
    #[serde(default)]
    pub starred_at: String,
}

/// On-disk envelope and the shape the UI reads.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFavoritesState {
    #[serde(default)]
    pub favorites: Vec<SkillFavorite>,
}

/// Canonical favorites path: `~/.agentic-hub/skills-favorites.json`.
pub fn default_path() -> PathBuf {
    home_dir()
        .join(".agentic-hub")
        .join("skills-favorites.json")
}

/// Dotfile-backed favorites store.
pub struct SkillFavoritesStore {
    path: PathBuf,
}

impl Default for SkillFavoritesStore {
    fn default() -> Self {
        SkillFavoritesStore {
            path: default_path(),
        }
    }
}

impl SkillFavoritesStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        SkillFavoritesStore { path: path.into() }
    }

    /// Read the favorites. Missing file → empty; malformed JSON → `StateParse`
    /// (never silently overwrites the user's file).
    pub fn read(&self) -> Result<SkillFavoritesState> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|e| CoreError::StateParse(e.to_string()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(SkillFavoritesState::default()),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    fn write(&self, state: &SkillFavoritesState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        // One-level backup before overwriting, so an accidental clobber of the
        // (optionally git-synced) favorites file is recoverable from `<file>.bak`.
        crate::paths::back_up_dotfile(&self.path);
        let json = serde_json::to_string_pretty(state)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Star a skill: upsert by `(provider, id)`, stamp `starred_at`, and pin it
    /// to the front (newest-first). Returns the stored favorite.
    pub fn add(&self, mut fav: SkillFavorite) -> Result<SkillFavorite> {
        fav.starred_at = now_iso8601();
        let mut state = self.read()?;
        state
            .favorites
            .retain(|f| !(f.provider == fav.provider && f.id == fav.id));
        state.favorites.insert(0, fav.clone());
        self.write(&state)?;
        Ok(fav)
    }

    /// Unstar a skill by `(provider, id)`. Absent entries are a no-op.
    pub fn remove(&self, provider: &str, id: &str) -> Result<()> {
        let mut state = self.read()?;
        state
            .favorites
            .retain(|f| !(f.provider == provider && f.id == id));
        self.write(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SkillFavoritesStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillFavoritesStore::with_path(dir.path().join("skills-favorites.json"));
        (dir, store)
    }

    fn fav(provider: &str, id: &str, name: &str) -> SkillFavorite {
        SkillFavorite {
            provider: provider.into(),
            id: id.into(),
            slug: id.rsplit('/').next().unwrap_or(id).into(),
            name: name.into(),
            source: id.rsplit_once('/').map(|(s, _)| s).unwrap_or("").into(),
            install_ref: id.into(),
            github_url: None,
            page_url: None,
            starred_at: String::new(),
        }
    }

    #[test]
    fn read_missing_file_is_empty() {
        let (_d, store) = store();
        assert!(store.read().unwrap().favorites.is_empty());
    }

    #[test]
    fn add_stamps_time_and_prepends_newest_first() {
        let (_d, store) = store();
        store.add(fav("skills.sh", "a/b/one", "One")).unwrap();
        let stored = store.add(fav("skills.sh", "a/b/two", "Two")).unwrap();
        assert!(!stored.starred_at.is_empty(), "starred_at is stamped");

        let list = store.read().unwrap().favorites;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a/b/two", "newest is first");
        assert_eq!(list[1].id, "a/b/one");
    }

    #[test]
    fn add_upserts_by_provider_and_id() {
        let (_d, store) = store();
        store.add(fav("skills.sh", "a/b/one", "Old name")).unwrap();
        store.add(fav("skills.sh", "a/b/one", "New name")).unwrap();
        let list = store.read().unwrap().favorites;
        assert_eq!(list.len(), 1, "same (provider,id) merges");
        assert_eq!(list[0].name, "New name");
    }

    #[test]
    fn same_id_different_provider_are_distinct() {
        let (_d, store) = store();
        store.add(fav("skills.sh", "a/b/one", "A")).unwrap();
        store.add(fav("other", "a/b/one", "B")).unwrap();
        assert_eq!(store.read().unwrap().favorites.len(), 2);
    }

    #[test]
    fn remove_drops_only_the_match() {
        let (_d, store) = store();
        store.add(fav("skills.sh", "a/b/one", "One")).unwrap();
        store.add(fav("skills.sh", "a/b/two", "Two")).unwrap();
        store.remove("skills.sh", "a/b/one").unwrap();
        let list = store.read().unwrap().favorites;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "a/b/two");
        // Removing an absent entry is a no-op.
        store.remove("skills.sh", "nope").unwrap();
        assert_eq!(store.read().unwrap().favorites.len(), 1);
    }

    #[test]
    fn write_keeps_a_backup_of_the_prior_good_file() {
        let (_d, store) = store();
        store.add(fav("skills.sh", "a/b/one", "One")).unwrap();
        let first = fs::read_to_string(&store.path).unwrap();

        // A later write backs the prior content up to `<file>.bak`.
        store.add(fav("skills.sh", "a/b/two", "Two")).unwrap();
        let mut bak = store.path.as_os_str().to_os_string();
        bak.push(".bak");
        assert_eq!(
            fs::read_to_string(std::path::PathBuf::from(bak)).unwrap(),
            first
        );
    }

    #[test]
    fn roundtrips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skills-favorites.json");
        let store = SkillFavoritesStore::with_path(dir.path().join("skills-favorites.json"));
        store.add(fav("skills.sh", "a/b/one", "One")).unwrap();
        // Reopen from the same path to prove it persisted.
        let reopened = SkillFavoritesStore::with_path(path).read().unwrap();
        assert_eq!(reopened.favorites.len(), 1);
        assert_eq!(reopened.favorites[0].name, "One");
    }
}
