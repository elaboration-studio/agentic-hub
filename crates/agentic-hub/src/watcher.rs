//! Filesystem watcher for the source roots. On a debounced batch of changes
//! (e.g. an external `git pull`), it re-scans, reconciles every enabled tool's
//! projections, re-patches the active workspace, and emits `sources-changed`
//! so the UI live-refreshes. We never run git — we only react to file events.
//!
//! The pure reconcile decision lives in `agentic-core`; this module owns the
//! OS subscription, debounce, and Tauri event bridge only.

use std::collections::HashSet;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use agentic_core::api;
use agentic_core::hook_sync;
use agentic_core::model::CapabilityItem;
use agentic_core::reconcile;
use agentic_core::settings::Settings;
use agentic_core::suite_store::SuiteStore;
use agentic_core::workspace_patch;
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use notify::{recommended_watcher, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// Quiet window after the last change before a reconcile fires. Coalesces the
/// burst of writes a `git pull` (or bulk edit) produces into a single pass.
const DEBOUNCE: Duration = Duration::from_millis(400);

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
    /// Start watching the configured source roots. Replaces any prior run.
    pub fn start(&self, app: AppHandle) {
        self.stop();
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

        // Seed the snapshot so the first change diffs against startup state
        // (no auto-enable flood on launch).
        let scanned = api::scan(&settings);
        let prev: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();

        let worker_app = app.clone();
        thread::spawn(move || worker_loop(rx, worker_app, prev));

        *self.inner.lock().expect("watcher lock") = Some(Running {
            _watcher: watcher,
            ctrl: tx,
        });
    }

    /// Stop watching. Idempotent.
    pub fn stop(&self) {
        if let Some(running) = self.inner.lock().expect("watcher lock").take() {
            let _ = running.ctrl.send(Msg::Stop);
            // Dropping `running` drops the watcher, ending OS events.
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
    /// No-op when the watcher is not running.
    pub fn restart_if_running(&self, app: AppHandle) {
        let running = self.inner.lock().expect("watcher lock").is_some();
        if running {
            self.start(app);
        }
    }
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

/// One reconcile pass: scan, reconcile every enabled tool, re-patch the active
/// workspace, then notify the UI. Best-effort — errors never crash the worker.
pub fn run_once(app: &AppHandle, prev: &mut HashSet<String>) {
    let Ok(settings) = Settings::load() else {
        return;
    };
    let scanned = api::scan(&settings);
    reconcile::reconcile_all(&scanned.items, &settings, prev);
    reapply_active_workspace(&settings, &scanned.items);
    *prev = scanned.items.iter().map(|i| i.id.clone()).collect();
    let _ = app.emit("sources-changed", ());
}

/// Full rescan + resync with no newcomer auto-enable (the Config fallback /
/// recovery path). Reconciles every enabled tool against current state and
/// re-patches the active workspace, then notifies the UI.
pub fn resync_now(app: &AppHandle) {
    let Ok(settings) = Settings::load() else {
        return;
    };
    let scanned = api::scan(&settings);
    // Pass the full current id set as "known" so nothing is treated as a
    // newcomer — a pure refresh/repair pass.
    let all: HashSet<String> = scanned.items.iter().map(|i| i.id.clone()).collect();
    reconcile::reconcile_all(&scanned.items, &settings, &all);
    reapply_active_workspace(&settings, &scanned.items);
    let _ = app.emit("sources-changed", ());
}

/// Replay the active workspace's recorded `lastApplied` patches so its hard
/// copies refresh from fresh source content. No-op when nothing is recorded.
fn reapply_active_workspace(settings: &Settings, items: &[CapabilityItem]) {
    let store = WorkspaceTargetStore::new();
    let Ok(Some(active)) = store.get_active() else {
        return;
    };
    if active.last_applied.is_empty() {
        return;
    }
    let suites = SuiteStore::with_path(settings.resolved_suites_path());
    let manifests = hook_sync::load_manifests(items);
    for applied in &active.last_applied {
        if let Ok(Some(suite)) = suites.get(&applied.suite_id) {
            let _ = workspace_patch::apply_workspace_patch(
                &active.dir,
                applied.tool_id,
                &suite,
                items,
                &manifests,
            );
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
