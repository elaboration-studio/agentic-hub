//! Stage 5: execute a plan. Partial-apply-tolerant — one failing op never
//! aborts the rest. Never removes a real (non-symlink, non-managed) file or
//! directory; such conflicts surface as errors. See `ARCHITECTURE.projection.md`.

use std::fs;
use std::path::Path;

use crate::managed_copy;
use crate::model::{ApplyError, ApplyResult, OperationKind, PlannedOperation};

#[derive(Debug, Clone, Copy)]
enum Effect {
    Created,
    Removed,
    Replaced,
    Refreshed,
    Skipped,
}

/// Apply each operation in order, invoking `on_progress(index, total, op, error)`
/// after each. Aggregates counts and per-op errors into an [`ApplyResult`].
pub fn apply<F>(ops: &[PlannedOperation], mut on_progress: F) -> ApplyResult
where
    F: FnMut(usize, usize, &PlannedOperation, Option<&ApplyError>),
{
    let total = ops.len();
    let mut result = ApplyResult::default();
    for (i, op) in ops.iter().enumerate() {
        let err = match execute(op) {
            Ok(effect) => {
                match effect {
                    Effect::Created => result.created += 1,
                    Effect::Removed => result.removed += 1,
                    Effect::Replaced => result.replaced += 1,
                    Effect::Refreshed => result.refreshed += 1,
                    Effect::Skipped => result.skipped += 1,
                }
                None
            }
            Err(e) => {
                result.errors.push(e.clone());
                Some(e)
            }
        };
        on_progress(i, total, op, err.as_ref());
    }
    result
}

/// Small internal failure, mapped to the large [`ApplyError`] once at the
/// `execute` boundary (keeps internal `Result`s cheap; see `clippy::result_large_err`).
enum Fail {
    Conflict,
    MissingSource,
    Io(std::io::Error),
}

// ApplyError intentionally embeds the full operation for UI display; this is
// the single boundary that surfaces it.
#[allow(clippy::result_large_err)]
fn execute(op: &PlannedOperation) -> Result<Effect, ApplyError> {
    run(op).map_err(|fail| to_apply_error(op, fail))
}

fn run(op: &PlannedOperation) -> Result<Effect, Fail> {
    use OperationKind::{
        ClearJsonSection, CreateLink, CreateManagedCopy, RemoveLink, RemoveManagedCopy,
        ReplaceLink, ReplaceManagedCopy, SkipConflict, SyncJsonSection,
    };
    let target = op.target_path.as_path();
    match op.kind {
        CreateLink => {
            let source = source(op)?;
            ensure_parent(target)?;
            symlink(&source, target)?;
            Ok(Effect::Created)
        }
        ReplaceLink => {
            let source = source(op)?;
            if op.force {
                // Confirmed take-over: remove any real file/dir/symlink first.
                managed_copy::remove_existing(target).map_err(Fail::Io)?;
            } else {
                if !is_symlink_or_managed_copy(target, &op.target_root) {
                    return Err(Fail::Conflict);
                }
                fs::remove_file(target).map_err(Fail::Io)?;
            }
            ensure_parent(target)?;
            symlink(&source, target)?;
            Ok(Effect::Replaced)
        }
        RemoveLink => {
            if !is_symlink(target) {
                return Err(Fail::Conflict);
            }
            fs::remove_file(target).map_err(Fail::Io)?;
            Ok(Effect::Removed)
        }
        CreateManagedCopy => {
            let source = source(op)?;
            managed_copy::write_managed_copy(&source, target, &op.target_root, &op.item_id, false)
                .map_err(Fail::Io)?;
            Ok(Effect::Created)
        }
        ReplaceManagedCopy => {
            let source = source(op)?;
            managed_copy::write_managed_copy(&source, target, &op.target_root, &op.item_id, true)
                .map_err(Fail::Io)?;
            Ok(Effect::Refreshed)
        }
        RemoveManagedCopy => {
            if !is_managed_copy(target, &op.target_root) {
                return Err(Fail::Conflict);
            }
            managed_copy::remove_managed_copy(target, &op.target_root).map_err(Fail::Io)?;
            Ok(Effect::Removed)
        }
        // Hooks are not yet wired into apply; planner does not emit these.
        SkipConflict | SyncJsonSection | ClearJsonSection => Ok(Effect::Skipped),
    }
}

fn to_apply_error(op: &PlannedOperation, fail: Fail) -> ApplyError {
    let (code, message) = match fail {
        Fail::Conflict => (
            "conflict_real_file_at_target",
            format!(
                "A real file or directory blocks {}",
                op.target_path.display()
            ),
        ),
        Fail::MissingSource => (
            "internal",
            "Operation requires a source path but none was provided".to_string(),
        ),
        Fail::Io(e) => ("internal", e.to_string()),
    };
    ApplyError {
        operation: op.clone(),
        message,
        code: code.to_string(),
    }
}

fn source(op: &PlannedOperation) -> Result<std::path::PathBuf, Fail> {
    op.source_path.clone().ok_or(Fail::MissingSource)
}

fn ensure_parent(target: &Path) -> Result<(), Fail> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(Fail::Io)?;
    }
    Ok(())
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn is_managed_copy(path: &Path, target_root: &Path) -> bool {
    !is_symlink(path) && managed_copy::read_entry(target_root, path).is_some()
}

fn is_symlink_or_managed_copy(path: &Path, target_root: &Path) -> bool {
    is_symlink(path) || managed_copy::read_entry(target_root, path).is_some()
}

#[cfg(unix)]
fn symlink(src: &Path, dst: &Path) -> Result<(), Fail> {
    std::os::unix::fs::symlink(src, dst).map_err(Fail::Io)
}

#[cfg(windows)]
fn symlink(src: &Path, dst: &Path) -> Result<(), Fail> {
    let res = if src.is_dir() {
        std::os::windows::fs::symlink_dir(src, dst)
    } else {
        std::os::windows::fs::symlink_file(src, dst)
    };
    res.map_err(Fail::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn op(kind: OperationKind, target: PathBuf, source: Option<PathBuf>) -> PlannedOperation {
        let target_root = target.parent().map(Path::to_path_buf).unwrap_or_default();
        PlannedOperation {
            tool: crate::model::ToolId::Codex,
            item_id: "agent:x.md".into(),
            target_root,
            target_path: target,
            source_path: source,
            kind,
            reason: String::new(),
            force: false,
        }
    }

    #[cfg(unix)]
    #[test]
    fn create_remove_link_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo");
        fs::create_dir_all(&source).unwrap();
        let target = dir.path().join("out/foo");

        let mut progress = 0;
        let res = apply(
            &[op(OperationKind::CreateLink, target.clone(), Some(source))],
            |_, _, _, _| progress += 1,
        );
        assert_eq!(res.created, 1);
        assert_eq!(progress, 1);
        assert!(is_symlink(&target));

        let res = apply(
            &[op(OperationKind::RemoveLink, target.clone(), None)],
            |_, _, _, _| {},
        );
        assert_eq!(res.removed, 1);
        assert!(!target.exists());
    }

    #[test]
    fn managed_copy_create_and_refuse_real_file_removal() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/agent.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "v1").unwrap();
        let target = dir.path().join("out/agent.md");

        let res = apply(
            &[op(
                OperationKind::CreateManagedCopy,
                target.clone(),
                Some(source),
            )],
            |_, _, _, _| {},
        );
        assert_eq!(res.created, 1);
        assert!(target.exists());
        assert!(managed_copy::read_entry(target.parent().unwrap(), &target).is_some());

        // A real (non-managed) file must not be removed.
        let real = dir.path().join("out/real.md");
        fs::write(&real, "user owned").unwrap();
        let res = apply(
            &[op(OperationKind::RemoveManagedCopy, real.clone(), None)],
            |_, _, _, _| {},
        );
        assert_eq!(res.errors.len(), 1);
        assert_eq!(res.errors[0].code, "conflict_real_file_at_target");
        assert!(real.exists());
    }

    #[cfg(unix)]
    #[test]
    fn force_replace_link_takes_over_real_file_and_dir() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "body").unwrap();

        // Real file at the target.
        let target = dir.path().join("out/foo.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "user owned").unwrap();

        let mut forced = op(
            OperationKind::ReplaceLink,
            target.clone(),
            Some(source.clone()),
        );
        forced.force = true;
        let res = apply(&[forced], |_, _, _, _| {});
        assert_eq!(res.replaced, 1);
        assert!(res.errors.is_empty());
        assert!(is_symlink(&target));

        // Real directory at the target is also taken over.
        let target_dir = dir.path().join("out/bar");
        fs::create_dir_all(target_dir.join("nested")).unwrap();
        let mut forced_dir = op(OperationKind::ReplaceLink, target_dir.clone(), Some(source));
        forced_dir.force = true;
        let res = apply(&[forced_dir], |_, _, _, _| {});
        assert_eq!(res.replaced, 1);
        assert!(is_symlink(&target_dir));
    }

    #[test]
    fn replace_link_without_force_refuses_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/foo.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "body").unwrap();
        let target = dir.path().join("out/foo.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "user owned").unwrap();

        let res = apply(
            &[op(OperationKind::ReplaceLink, target.clone(), Some(source))],
            |_, _, _, _| {},
        );
        assert_eq!(res.errors.len(), 1);
        assert_eq!(res.errors[0].code, "conflict_real_file_at_target");
        // The real file is untouched.
        assert_eq!(fs::read_to_string(&target).unwrap(), "user owned");
    }

    #[test]
    fn force_replace_managed_copy_takes_over_real_dir() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src/skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# skill").unwrap();

        let root = dir.path().join("out");
        let target = root.join("skill");
        // Real user dir at the target.
        fs::create_dir_all(target.join("mine")).unwrap();

        let mut forced = op(
            OperationKind::ReplaceManagedCopy,
            target.clone(),
            Some(source),
        );
        forced.target_root = root.clone();
        forced.force = true;
        let res = apply(&[forced], |_, _, _, _| {});
        assert_eq!(res.refreshed, 1);
        assert!(res.errors.is_empty());
        assert!(target.join("SKILL.md").is_file());
        // The user's pre-existing content is gone (destructive take-over).
        assert!(!target.join("mine").exists());
        assert!(managed_copy::read_entry(&root, &target).is_some());
    }

    #[test]
    fn skip_conflict_counts_skipped() {
        let res = apply(
            &[op(
                OperationKind::SkipConflict,
                PathBuf::from("/nope"),
                None,
            )],
            |_, _, _, _| {},
        );
        assert_eq!(res.skipped, 1);
        assert!(res.errors.is_empty());
    }
}
