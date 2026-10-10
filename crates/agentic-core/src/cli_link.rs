//! Keeps `~/.agentic-hub/bin/ehub` a symlink to the running app binary so the
//! headless CLI works with the app closed. A real file or directory at the
//! link path is never replaced (same rule as projection). Unix only.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// What [`ensure`] did to the link path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    Created,
    Retargeted { previous: PathBuf },
    Unchanged,
    /// A real file or directory sits at the link path; it was left alone.
    Conflict,
}

/// `~/.agentic-hub/bin/ehub`.
pub fn default_link_path() -> PathBuf {
    crate::paths::home_dir()
        .join(".agentic-hub")
        .join("bin")
        .join("ehub")
}

/// Make `link` a symlink to `exe`. A retarget swaps in a sibling temp link
/// with one `rename`, so readers never see the path missing.
pub fn ensure(exe: &Path, link: &Path) -> Result<LinkOutcome> {
    let meta = match fs::symlink_metadata(link) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            if let Some(parent) = link.parent() {
                fs::create_dir_all(parent)?;
            }
            symlink(exe, link)?;
            return Ok(LinkOutcome::Created);
        }
        Err(e) => return Err(e.into()),
    };
    if !meta.file_type().is_symlink() {
        return Ok(LinkOutcome::Conflict);
    }
    let previous = fs::read_link(link)?;
    if previous == exe {
        return Ok(LinkOutcome::Unchanged);
    }
    let tmp = link.with_file_name(format!(".ehub.tmp-{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    symlink(exe, &tmp)?;
    if let Err(e) = fs::rename(&tmp, link) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(LinkOutcome::Retargeted { previous })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("app").join("agentic-hub");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"bin").unwrap();
        let link = dir.path().join("home").join(".agentic-hub").join("bin").join("ehub");
        (dir, exe, link)
    }

    #[test]
    fn creates_parent_and_symlink_when_missing() {
        let (_dir, exe, link) = setup();

        let outcome = ensure(&exe, &link).unwrap();

        assert_eq!(outcome, LinkOutcome::Created);
        assert_eq!(fs::read_link(&link).unwrap(), exe);
    }

    #[test]
    fn leaves_correct_symlink_unchanged() {
        let (_dir, exe, link) = setup();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&exe, &link).unwrap();

        let outcome = ensure(&exe, &link).unwrap();

        assert_eq!(outcome, LinkOutcome::Unchanged);
        assert_eq!(fs::read_link(&link).unwrap(), exe);
    }

    #[test]
    fn retargets_symlink_pointing_elsewhere() {
        let (dir, exe, link) = setup();
        let old = dir.path().join("old-app");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&old, &link).unwrap();

        let outcome = ensure(&exe, &link).unwrap();

        assert_eq!(outcome, LinkOutcome::Retargeted { previous: old });
        assert_eq!(fs::read_link(&link).unwrap(), exe);
        let leftovers: Vec<_> = fs::read_dir(link.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("ehub")]);
    }

    #[test]
    fn leaves_real_file_untouched() {
        let (_dir, exe, link) = setup();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(&link, b"user script").unwrap();

        let outcome = ensure(&exe, &link).unwrap();

        assert_eq!(outcome, LinkOutcome::Conflict);
        assert_eq!(fs::read(&link).unwrap(), b"user script");
    }

    #[test]
    fn leaves_real_directory_untouched() {
        let (_dir, exe, link) = setup();
        fs::create_dir_all(link.join("inner")).unwrap();

        let outcome = ensure(&exe, &link).unwrap();

        assert_eq!(outcome, LinkOutcome::Conflict);
        assert!(link.join("inner").is_dir());
    }

    #[test]
    fn errors_when_parent_is_a_file() {
        let (_dir, exe, link) = setup();
        let bin = link.parent().unwrap();
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(bin, b"not a dir").unwrap();

        assert!(ensure(&exe, &link).is_err());
    }
}
