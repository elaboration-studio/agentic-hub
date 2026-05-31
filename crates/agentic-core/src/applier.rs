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
            if !is_symlink_or_managed_copy(target) {
                return Err(Fail::Conflict);
            }
            let source = source(op)?;
            fs::remove_file(target).map_err(Fail::Io)?;
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
            managed_copy::write_managed_copy(&source, target, false).map_err(Fail::Io)?;
            Ok(Effect::Created)
        }
        ReplaceManagedCopy => {
            let source = source(op)?;
            managed_copy::write_managed_copy(&source, target, true).map_err(Fail::Io)?;
            Ok(Effect::Refreshed)
        }
        RemoveManagedCopy => {
            if !is_managed_copy(target) {
                return Err(Fail::Conflict);
            }
            managed_copy::remove_managed_copy(target).map_err(Fail::Io)?;
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

fn is_managed_copy(path: &Path) -> bool {
    !is_symlink(path) && managed_copy::read_meta(path).is_some()
}

fn is_symlink_or_managed_copy(path: &Path) -> bool {
    is_symlink(path) || managed_copy::read_meta(path).is_some()
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
        PlannedOperation {
            tool: crate::model::ToolId::Codex,
            item_id: "skill:x".into(),
            target_path: target,
            source_path: source,
            kind,
            reason: String::new(),
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
            &[op(
                OperationKind::CreateLink,
                target.clone(),
                Some(source.clone()),
            )],
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
                Some(source.clone()),
            )],
            |_, _, _, _| {},
        );
        assert_eq!(res.created, 1);
        assert!(target.exists());
        assert!(managed_copy::read_meta(&target).is_some());

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
