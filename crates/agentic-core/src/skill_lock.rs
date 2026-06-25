//! Read the skills.sh project lock file (`skills-lock.json`).
//!
//! skills.sh records project-scoped installs in `<workspace>/skills-lock.json`,
//! mapping each install name (the skill's folder name) to its source. The hub
//! reads it (read-only) to mark which workspace skills the skills.sh CLI manages
//! so it can offer a one-click `npx skills update`. Parsing is **tolerant**: a
//! missing or malformed lock yields no managed skills and never fails the
//! read-only inventory scan. See `docs/tech/modules/skill-sources.md`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// File name skills.sh writes at the project root for project-scoped installs.
pub const LOCAL_LOCK_FILE: &str = "skills-lock.json";

/// One entry in the project lock's `skills` map. Only the fields we need are
/// kept; unknown ones (`skillPath`, `computedHash`, …) are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedSkillEntry {
    /// The source the skill was installed from — the `owner/repo` ref handed
    /// back to `npx skills` (or another transport's source string).
    #[serde(default)]
    pub source: String,
    /// Transport hint: `github`, `local`, etc.
    #[serde(default)]
    pub source_type: String,
}

/// A parsed `skills-lock.json`. `skills` maps an install name (the skill's
/// folder name) to its source metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LocalSkillLock {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, LockedSkillEntry>,
}

/// Parse a `skills-lock.json` body. Pure and unit-tested.
pub fn parse_local_lock(body: &str) -> Result<LocalSkillLock, serde_json::Error> {
    serde_json::from_str(body)
}

/// Read `<ws>/skills-lock.json`, if present and parseable. Tolerant: a missing
/// file or malformed JSON yields `None`, so a third-party lock file can never
/// break the read-only inventory scan.
pub fn read_local_lock(ws: &Path) -> Option<LocalSkillLock> {
    let body = std::fs::read_to_string(ws.join(LOCAL_LOCK_FILE)).ok()?;
    parse_local_lock(&body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real skills.sh shape, including extra fields we ignore.
    const REAL_LOCK: &str = r#"{
        "version": 1,
        "skills": {
            "rust-best-practices": {
                "source": "apollographql/skills",
                "sourceType": "github",
                "skillPath": "skills/rust-best-practices/SKILL.md",
                "computedHash": "fd336f2f"
            },
            "vercel-react-best-practices": {
                "source": "vercel-labs/agent-skills",
                "sourceType": "github",
                "skillPath": "skills/react-best-practices/SKILL.md",
                "computedHash": "ca7b0c0c"
            }
        }
    }"#;

    #[test]
    fn parses_real_lock_and_ignores_unknown_fields() {
        let lock = parse_local_lock(REAL_LOCK).unwrap();
        assert_eq!(lock.version, 1);
        assert_eq!(lock.skills.len(), 2);
        let rust = lock.skills.get("rust-best-practices").unwrap();
        assert_eq!(rust.source, "apollographql/skills");
        assert_eq!(rust.source_type, "github");
        assert_eq!(
            lock.skills
                .get("vercel-react-best-practices")
                .unwrap()
                .source,
            "vercel-labs/agent-skills"
        );
    }

    #[test]
    fn parses_empty_lock() {
        let lock = parse_local_lock(r#"{"version":1,"skills":{}}"#).unwrap();
        assert!(lock.skills.is_empty());
        // A bare object also parses (every field defaults).
        assert!(parse_local_lock("{}").unwrap().skills.is_empty());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_local_lock("not json").is_err());
    }

    #[test]
    fn read_local_lock_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_local_lock(dir.path()).is_none());
    }

    #[test]
    fn read_local_lock_malformed_is_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOCAL_LOCK_FILE), "not json").unwrap();
        assert!(read_local_lock(dir.path()).is_none());
    }

    #[test]
    fn read_local_lock_reads_present_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOCAL_LOCK_FILE), REAL_LOCK).unwrap();
        let lock = read_local_lock(dir.path()).unwrap();
        assert!(lock.skills.contains_key("rust-best-practices"));
    }
}
