//! Filesystem watcher for the source roots and the active workspace. On a
//! debounced batch of changes (e.g. an external `git pull` or an edit inside a
//! workspace's tool dirs), it re-scans, reconciles every enabled tool's global
//! projections, and emits `sources-changed` + `workspace-changed` so the UI
//! live-refreshes both the global matrix and the read-only workspace inventory.
//! We never run git — we only react to file events.
//!
//! The pure reconcile decision lives in `agentic-core`; this module owns the
//! OS subscription, debounce, and Tauri event bridge only.

use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;

use agentic_core::api;
use agentic_core::reconcile;
use agentic_core::settings::Settings;
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use notify::{recommended_watcher, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::commands::SuiteStoreChangedEvent;

/// The per-tool directories and instruction files inside a workspace that the
/// inventory scan reads; watched (when present) so edits live-refresh the view.
const WORKSPACE_WATCH_DIRS: [&str; 7] = [
    ".agents", ".agent", ".claude", ".cursor", ".codex", ".kiro", ".github",
];
const WORKSPACE_WATCH_FILES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];

/// Quiet window after the last change before a reconcile fires. Coalesces the
/// burst of writes a `git pull` (or bulk edit) produces into a single pass.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// Process-wide guard serializing every projection transaction: watcher
/// reconciliation, explicit suite apply, binding recovery, and suite mutation
/// re-sync. Poison-tolerant so a panicked holder cannot brick future writes.
fn projection_transaction_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Run `f` while holding the process-wide projection transaction guard.
/// Callers keep state-file read, filesystem apply, and binding record inside
/// this closure so separate Tauri windows cannot interleave those phases.
pub fn with_projection_transaction<T>(f: impl FnOnce() -> T) -> T {
    let _guard = projection_transaction_guard();
    f()
}

/// Control/event messages from the OS watcher callback to the worker thread.
enum Msg {
    Changed,
    Stop,
}

struct Running {
    /// Held to keep the OS subscription alive; dropping it stops events.
    _watcher: RecommendedWatcher,
    /// Shared with the watcher callback; used to signal the worker to stop.
    ctrl: Sender<Msg>,
}

/// Thread-safe handle to the (optional) running watcher, held in Tauri state.
#[derive(Default)]
pub struct WatcherState {
    inner: Mutex<Option<Running>>,
}

impl WatcherState {
    /// Poison-tolerant lock of the singleton slot. Never panics on a poisoned
    /// mutex so a stray panic in one holder cannot brick the watcher controls.
    fn lock(&self) -> MutexGuard<'_, Option<Running>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start watching the configured source roots. Replaces any prior run.
    pub fn start(&self, app: AppHandle) {
        let mut slot = self.lock();
        replace(&mut slot, app);
    }

    /// Stop watching. Idempotent.
    pub fn stop(&self) {
        if let Some(running) = self.lock().take() {
            let _ = running.ctrl.send(Msg::Stop);
            // Dropping `running` drops the watcher, ending OS events; the worker
            // sees the channel disconnect (or the Stop) and exits.
        }
    }

    /// Turn the watcher on or off (used by the toggle command).
    pub fn set_enabled(&self, app: AppHandle, enabled: bool) {
        if enabled {
            self.start(app);
        } else {
            self.stop();
        }
    }

    /// Re-subscribe to the current source roots (after sources/settings change).
    /// No-op when the watcher is not running. Checked and applied while holding
    /// the slot lock so concurrent callers cannot spawn duplicate watchers.
    pub fn restart_if_running(&self, app: AppHandle) {
        let mut slot = self.lock();
        if slot.is_some() {
            replace(&mut slot, app);
        }
    }
}

/// Replace the current run with a fresh one, holding the singleton slot lock for
/// the whole swap so exactly one watcher + worker ever exists. The caller owns
/// the lock guard; this never re-locks (the mutex is not reentrant).
fn replace(slot: &mut MutexGuard<'_, Option<Running>>, app: AppHandle) {
    // Tear down the prior run first.
    if let Some(old) = slot.take() {
        let _ = old.ctrl.send(Msg::Stop);
        drop(old);
    }

    let Ok(settings) = Settings::load() else {
        return;
    };

    let (tx, rx) = mpsc::channel::<Msg>();
    let tx_evt = tx.clone();
    let mut watcher = match recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if is_relevant(&event) {
                let _ = tx_evt.send(Msg::Changed);
            }
        }
    }) {
        Ok(w) => w,
        Err(_) => return,
    };

    for source in settings.resolve_sources() {
        if source.path.is_dir() {
            let _ = watcher.watch(&source.path, RecursiveMode::Recursive);
        }
    }
    watch_active_workspace(&mut watcher);
    watch_state_files(&mut watcher, &settings);

    // Seed the snapshot so the first change diffs against startup state
    // (no auto-enable flood on launch).
    let scanned = api::scan(&settings);
    let prev: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();

    thread::spawn(move || worker_loop(rx, app, prev));

    **slot = Some(Running {
        _watcher: watcher,
        ctrl: tx,
    });
}

/// Worker thread: block for the first change, coalesce the burst over a quiet
/// window, then reconcile once. `prev` carries the last-known item-id set so
/// newcomers can be detected across cycles.
fn worker_loop(rx: mpsc::Receiver<Msg>, app: AppHandle, mut prev: HashSet<String>) {
    loop {
        match rx.recv() {
            Ok(Msg::Changed) => {
                // Drain until the stream is quiet for DEBOUNCE.
                loop {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(Msg::Changed) => continue,
                        Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
                run_once(&app, &mut prev);
            }
            Ok(Msg::Stop) | Err(_) => return,
        }
    }
}

/// One reconcile pass: scan, reconcile every enabled tool, then notify the UI.
/// Emits both `sources-changed` (global matrix) and `workspace-changed`
/// (read-only inventory) since a single batch can touch either watched tree.
/// Best-effort — errors never crash the worker. Holds the process-wide
/// projection transaction so it never overlaps another write pass.
fn run_once(app: &AppHandle, prev: &mut HashSet<String>) {
    let _guard = projection_transaction_guard();
    let Ok(settings) = Settings::load() else {
        return;
    };
    let scanned = api::scan(&settings);
    reconcile::reconcile_all(&scanned.items, &settings, prev);
    *prev = scanned.items.iter().map(|i| i.id.clone()).collect();
    emit_refresh(app);
}

/// Full rescan + resync with no newcomer auto-enable (the Config fallback /
/// recovery path). Reconciles every enabled tool against current state, then
/// notifies the UI. Serialized against the watcher worker and suite writes via
/// the process-wide projection transaction.
pub fn resync_now(app: &AppHandle) {
    let _guard = projection_transaction_guard();
    let Ok(settings) = Settings::load() else {
        return;
    };
    let scanned = api::scan(&settings);
    // Pass the full current id set as "known" so nothing is treated as a
    // newcomer — a pure refresh/repair pass.
    let all: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
    reconcile::reconcile_all(&scanned.items, &settings, &all);
    emit_refresh(app);
}

/// Notify the UI after a reconcile/refresh pass. Beyond the global matrix and
/// workspace inventory, we also nudge the suites and favorites views: a single
/// batch (e.g. a `git pull`) can rewrite the synced suites or favorites file, so
/// both must reload from disk to avoid showing — and later re-saving — a stale
/// in-memory snapshot.
fn emit_refresh(app: &AppHandle) {
    let _ = app.emit("sources-changed", ());
    let _ = app.emit("workspace-changed", ());
    let _ = app.emit(
        "suite-store-changed",
        SuiteStoreChangedEvent {
            kind: "external".to_string(),
            suite_id: None,
        },
    );
    let _ = app.emit("skills-favorites-changed", ());
}

/// Subscribe to the active workspace's existing tool dirs and instruction files
/// so edits there live-refresh the inventory. Lightweight: only the per-tool
/// folders, never the whole repo. No-op when no workspace is active.
fn watch_active_workspace(watcher: &mut RecommendedWatcher) {
    let Ok(Some(active)) = WorkspaceTargetStore::new().get_active() else {
        return;
    };
    let dir: &Path = &active.dir;
    for sub in WORKSPACE_WATCH_DIRS {
        let path = dir.join(sub);
        if path.is_dir() {
            let _ = watcher.watch(&path, RecursiveMode::Recursive);
        }
    }
    for file in WORKSPACE_WATCH_FILES {
        let path = dir.join(file);
        if path.is_file() {
            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        }
    }
}

/// Subscribe to the resolved suites and skill-favorites files so an external
/// rewrite (a `git pull` on a synced custom path) wakes the worker and the UI
/// reloads from disk. Watched as single files (NonRecursive): when they live at
/// a custom path inside a git repo we must not watch the whole repo. No-op for a
/// file that does not exist yet.
fn watch_state_files(watcher: &mut RecommendedWatcher, settings: &Settings) {
    for path in [
        settings.resolved_suites_path(),
        settings.resolved_favorites_path(),
    ] {
        if path.is_file() {
            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        }
    }
}

/// Skip events confined to a `.git/` internal directory (index/lock churn).
/// A `git pull` also rewrites working-tree files, which remain relevant.
fn is_relevant(event: &Event) -> bool {
    if event.paths.is_empty() {
        return true;
    }
    event.paths.iter().any(|p| {
        !p.components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn workspace_watch_dirs_cover_all_inventory_roots() {
        for expected in [
            ".agents", ".agent", ".claude", ".cursor", ".codex", ".kiro", ".github",
        ] {
            assert!(
                WORKSPACE_WATCH_DIRS.contains(&expected),
                "missing workspace inventory root: {expected}"
            );
        }
    }

    #[test]
    fn projection_transaction_serializes_separate_window_writes() {
        let (first_entered_tx, first_entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first = std::thread::spawn(move || {
            with_projection_transaction(|| {
                first_entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
        });
        first_entered_rx.recv().unwrap();

        let (second_ready_tx, second_ready_rx) = mpsc::channel();
        let (second_entered_tx, second_entered_rx) = mpsc::channel();
        let second = std::thread::spawn(move || {
            second_ready_tx.send(()).unwrap();
            with_projection_transaction(|| second_entered_tx.send(()).unwrap());
        });

        second_ready_rx.recv().unwrap();
        assert!(second_entered_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err());
        release_tx.send(()).unwrap();
        second_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        first.join().unwrap();
        second.join().unwrap();
    }
}
