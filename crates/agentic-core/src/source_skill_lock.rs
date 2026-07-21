//! Read/write the skills.sh lock file at a Hub **source root**
//! (`<sourceRoot>/skills-lock.json`) — the library-install counterpart to the
//! per-workspace lock read (read-only) in `skill_lock.rs`. A library install
//! stages `npx skills add` in a scratch directory, copies the produced skill
//! folder into the shared-root contract layout
//! (`<sourceRoot>/skills/<destSubpath>/<name>/`), and records provenance here
//! so the skill is Hub-tracked and updatable. Schema matches the real
//! skills.sh lock shape observed in the wild (`version`, `skills: { name: {
//! source, sourceType, skillPath, computedHash } }`). See
//! `docs/tech/modules/skill-sources.md`.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::managed_copy::{content_hash, copy_dir, remove_existing};
use crate::skill_source::validate_skill_slug;

/// File name at the source root — same name skills.sh writes at a project root.
pub const SOURCE_LOCK_FILE: &str = "skills-lock.json";

/// One entry in the source-root lock's `skills` map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLockEntry {
    /// The `owner/repo` (or `owner/repo/skill`) ref the skill was installed from.
    pub source: String,
    /// Transport hint: `github` today.
    pub source_type: String,
    /// Path to the skill's `SKILL.md`, relative to the source root, unix
    /// separators (e.g. `skills/arno/cmo/ad-creative/SKILL.md`).
    pub skill_path: String,
    /// Hex sha256 of the installed `SKILL.md`, for update/staleness checks.
    pub computed_hash: String,
}

/// A parsed source-root `skills-lock.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSkillLock {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, SourceLockEntry>,
}

fn default_version() -> u32 {
    1
}

impl Default for SourceSkillLock {
    fn default() -> Self {
        SourceSkillLock {
            version: 1,
            skills: BTreeMap::new(),
        }
    }
}

/// Read `<sourceRoot>/skills-lock.json`. A missing file is an empty lock;
/// malformed JSON is a typed error so a corrupt lock never silently loses the
/// user's install history on the next write.
pub fn read_source_lock(source_root: &Path) -> Result<SourceSkillLock> {
    match std::fs::read_to_string(source_root.join(SOURCE_LOCK_FILE)) {
        Ok(body) => {
            serde_json::from_str(&body).map_err(|e| CoreError::StateParse(e.to_string()))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(SourceSkillLock::default()),
        Err(e) => Err(CoreError::Io(e)),
    }
}

/// Upsert one entry by install name (the lock key).
pub fn upsert_entry(lock: &mut SourceSkillLock, name: &str, entry: SourceLockEntry) {
    lock.skills.insert(name.to_string(), entry);
}

/// Write the lock atomically (tmp + rename), preceded by a one-level backup of
/// the prior good file so an accidental clobber is recoverable from
/// `skills-lock.json.bak`.
pub fn write_source_lock(source_root: &Path, lock: &SourceSkillLock) -> Result<()> {
    std::fs::create_dir_all(source_root)?;
    let path = source_root.join(SOURCE_LOCK_FILE);
    crate::paths::back_up_dotfile(&path);
    let json = serde_json::to_string_pretty(lock)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// True when `sub` is a safe destination subpath under `<sourceRoot>/skills/`:
/// empty (skill lands directly at `skills/<name>/`), or every `/`-separated
/// component is non-empty, not `.`/`..`, and made only of
/// `[A-Za-z0-9._-]`. Rejects a leading slash so the path can never be read as
/// absolute. This is the guard before the subpath is joined onto a real path.
pub fn validate_dest_subpath(sub: &str) -> bool {
    if sub.is_empty() {
        return true;
    }
    if sub.starts_with('/') || sub.starts_with('\\') {
        return false;
    }
    sub.split(['/', '\\']).all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

/// One skill successfully normalized into a source root's contract layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSkill {
    pub name: String,
    /// Relative to `source_root`, unix separators.
    pub skill_path: String,
    pub computed_hash: String,
}

/// Copy `staging_skill_dir` (the folder `locate_installed_skill` found) into
/// `<source_root>/skills/<dest_subpath>/<name>/`, replacing whatever the Hub
/// previously placed there (the update path), and return the resulting
/// relative `skillPath` + content hash for the lock entry. Validates
/// `dest_subpath` and `name` and re-confirms the resolved path stays inside
/// `<source_root>/skills/` before touching disk — the last line of defense
/// against a path-traversal destination reaching the filesystem.
pub fn normalize_into_source_root(
    staging_skill_dir: &Path,
    source_root: &Path,
    dest_subpath: &str,
    name: &str,
) -> Result<InstalledSkill> {
    if !validate_dest_subpath(dest_subpath) {
        return Err(CoreError::InvalidSkillRef(dest_subpath.to_string()));
    }
    if !validate_skill_slug(name) {
        return Err(CoreError::InvalidSkillRef(name.to_string()));
    }
    let skills_root = source_root.join("skills");
    let dest_dir = if dest_subpath.is_empty() {
        skills_root.join(name)
    } else {
        skills_root.join(dest_subpath).join(name)
    };
    if !dest_dir.starts_with(&skills_root) {
        return Err(CoreError::InvalidSkillRef(dest_subpath.to_string()));
    }

    if let Some(parent) = dest_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_existing(&dest_dir)?;
    copy_dir(staging_skill_dir, &dest_dir)?;

    let computed_hash = content_hash(&dest_dir).unwrap_or_default();
    let skill_path = relative_unix(source_root, &dest_dir.join("SKILL.md"));
    Ok(InstalledSkill {
        name: name.to_string(),
        skill_path,
        computed_hash,
    })
}

/// `target` relative to `base`, rendered with unix separators regardless of
/// platform, for lock `skillPath` values that must match the real skills.sh
/// lock format.
fn relative_unix(base: &Path, target: &Path) -> String {
    let rel: PathBuf = target.strip_prefix(base).unwrap_or(target).to_path_buf();
    rel.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, skill_path: &str) -> SourceLockEntry {
        SourceLockEntry {
            source: source.to_string(),
            source_type: "github".to_string(),
            skill_path: skill_path.to_string(),
            computed_hash: "deadbeef".to_string(),
        }
    }

    #[test]
    fn read_missing_lock_is_empty_default() {
        let dir = tempfile::tempdir().unwrap();
        let lock = read_source_lock(dir.path()).unwrap();
        assert_eq!(lock.version, 1);
        assert!(lock.skills.is_empty());
    }

    #[test]
    fn read_malformed_lock_errors_instead_of_silently_dropping() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SOURCE_LOCK_FILE), "not json").unwrap();
        assert!(matches!(
            read_source_lock(dir.path()).unwrap_err(),
            CoreError::StateParse(_)
        ));
    }

    #[test]
    fn parses_the_real_skills_sh_lock_shape() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SOURCE_LOCK_FILE),
            r#"{
                "version": 1,
                "skills": {
                    "rust-best-practices": {
                        "source": "apollographql/skills",
                        "sourceType": "github",
                        "skillPath": "skills/rust-best-practices/SKILL.md",
                        "computedHash": "fd336f2f"
                    }
                }
            }"#,
        )
        .unwrap();
        let lock = read_source_lock(dir.path()).unwrap();
        let e = lock.skills.get("rust-best-practices").unwrap();
        assert_eq!(e.source, "apollographql/skills");
        assert_eq!(e.skill_path, "skills/rust-best-practices/SKILL.md");
        assert_eq!(e.computed_hash, "fd336f2f");
    }

    #[test]
    fn upsert_merges_by_name_and_write_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = read_source_lock(dir.path()).unwrap();
        upsert_entry(
            &mut lock,
            "ad-creative",
            entry("coreyhaines31/marketingskills", "skills/arno/cmo/ad-creative/SKILL.md"),
        );
        write_source_lock(dir.path(), &lock).unwrap();

        let reread = read_source_lock(dir.path()).unwrap();
        assert_eq!(reread.skills.len(), 1);
        assert_eq!(
            reread.skills["ad-creative"].skill_path,
            "skills/arno/cmo/ad-creative/SKILL.md"
        );

        // A later upsert of the same name replaces, not duplicates.
        let mut lock2 = reread;
        upsert_entry(
            &mut lock2,
            "ad-creative",
            entry("coreyhaines31/marketingskills", "skills/other/ad-creative/SKILL.md"),
        );
        write_source_lock(dir.path(), &lock2).unwrap();
        let final_lock = read_source_lock(dir.path()).unwrap();
        assert_eq!(final_lock.skills.len(), 1);
        assert_eq!(
            final_lock.skills["ad-creative"].skill_path,
            "skills/other/ad-creative/SKILL.md"
        );
    }

    #[test]
    fn write_keeps_a_backup_of_the_prior_good_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = SourceSkillLock::default();
        upsert_entry(&mut lock, "one", entry("a/b", "skills/one/SKILL.md"));
        write_source_lock(dir.path(), &lock).unwrap();
        let first = std::fs::read_to_string(dir.path().join(SOURCE_LOCK_FILE)).unwrap();

        upsert_entry(&mut lock, "two", entry("a/b", "skills/two/SKILL.md"));
        write_source_lock(dir.path(), &lock).unwrap();

        let bak = dir.path().join(format!("{SOURCE_LOCK_FILE}.bak"));
        assert_eq!(std::fs::read_to_string(bak).unwrap(), first);
    }

    #[test]
    fn validate_dest_subpath_accepts_empty_and_nested() {
        assert!(validate_dest_subpath(""));
        assert!(validate_dest_subpath("arno/cmo"));
        assert!(validate_dest_subpath("dev.tools_v2"));
    }

    #[test]
    fn validate_dest_subpath_rejects_unsafe() {
        assert!(!validate_dest_subpath("/etc"), "absolute");
        assert!(!validate_dest_subpath("../escape"), "parent traversal");
        assert!(!validate_dest_subpath("arno/../../etc"), "embedded traversal");
        assert!(!validate_dest_subpath("a//b"), "empty component");
        assert!(!validate_dest_subpath("a;rm -rf /"), "shell metachars");
    }

    fn staged_skill(root: &Path, name: &str, body: &str) -> PathBuf {
        let dir = root.join(".claude/skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), body).unwrap();
        std::fs::write(dir.join("references.md"), "extra file").unwrap();
        dir
    }

    #[test]
    fn normalize_copies_into_contract_layout_with_dest_subpath() {
        let staging = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let staged = staged_skill(staging.path(), "ad-creative", "# ad-creative");

        let installed =
            normalize_into_source_root(&staged, source_root.path(), "arno/cmo", "ad-creative")
                .unwrap();

        let dest = source_root.path().join("skills/arno/cmo/ad-creative");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("references.md").is_file());
        assert_eq!(installed.name, "ad-creative");
        assert_eq!(installed.skill_path, "skills/arno/cmo/ad-creative/SKILL.md");
        assert_eq!(installed.computed_hash, content_hash(&dest).unwrap());
    }

    #[test]
    fn normalize_lands_directly_under_skills_when_dest_subpath_empty() {
        let staging = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let staged = staged_skill(staging.path(), "flat-skill", "# flat");

        let installed =
            normalize_into_source_root(&staged, source_root.path(), "", "flat-skill").unwrap();

        assert_eq!(installed.skill_path, "skills/flat-skill/SKILL.md");
        assert!(source_root
            .path()
            .join("skills/flat-skill/SKILL.md")
            .is_file());
    }

    #[test]
    fn normalize_replaces_prior_copy_for_update() {
        let staging = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let v1 = staged_skill(staging.path(), "dev-skill", "# v1");
        normalize_into_source_root(&v1, source_root.path(), "dev", "dev-skill").unwrap();

        // Re-stage a v2 elsewhere and normalize again at the same destination —
        // the update flow re-runs the same staged install, not a diff/patch.
        let staging2 = tempfile::tempdir().unwrap();
        let v2 = staged_skill(staging2.path(), "dev-skill", "# v2 changed");
        let installed =
            normalize_into_source_root(&v2, source_root.path(), "dev", "dev-skill").unwrap();

        let dest_dir = source_root.path().join("skills/dev/dev-skill");
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("SKILL.md")).unwrap(),
            "# v2 changed"
        );
        assert_eq!(installed.computed_hash, content_hash(&dest_dir).unwrap());
    }

    #[test]
    fn normalize_rejects_path_traversal_dest_subpath() {
        let staging = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let staged = staged_skill(staging.path(), "evil", "# evil");

        let err = normalize_into_source_root(&staged, source_root.path(), "../../etc", "evil")
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidSkillRef(_)));
        assert!(!source_root.path().parent().unwrap().join("etc").exists());
    }

    #[test]
    fn normalize_rejects_unsafe_skill_name() {
        let staging = tempfile::tempdir().unwrap();
        let source_root = tempfile::tempdir().unwrap();
        let staged = staged_skill(staging.path(), "ok-name", "# ok");

        let err =
            normalize_into_source_root(&staged, source_root.path(), "", "../escape").unwrap_err();
        assert!(matches!(err, CoreError::InvalidSkillRef(_)));
    }
}
