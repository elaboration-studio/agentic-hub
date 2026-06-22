//! Dedicated, live-streaming skill-install window + its commands.
//!
//! The workspace FAB opens a real `install` window (not a modal) that loads the
//! starred-skill matrix, runs `npx skills add` per selected skill while
//! streaming stdout/stderr live over a Tauri [`Channel`], and exposes a Cancel
//! that kills the in-flight child. This is the **only** workspace write path
//! (the inventory scan stays read-only); the target dir is always resolved from
//! the workspace store, never an arbitrary path, and the argv is a fixed,
//! validated vector — see `docs/tech/modules/skill-sources.md`.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use agentic_core::error::CoreError;
use agentic_core::model::ToolId;
use agentic_core::skill_source::{
    provider_for, skills_install_command, skills_npx_args, skills_update_command,
    skills_update_npx_args, validate_install_ref, validate_skill_slug, SkillInstallEvent,
};
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::IpcError;
use crate::watcher::WatcherState;

type IpcResult<T> = Result<T, IpcError>;

/// Label of the install window.
pub const INSTALL_LABEL: &str = "install";

const INSTALL_WIDTH: f64 = 760.0;
const INSTALL_HEIGHT: f64 = 620.0;

/// A single skill the window should **update** (rather than install). Set when
/// the window is opened from a workspace row's "Update via skills.sh" action; the
/// window then runs `npx skills update <name>` instead of showing the install
/// matrix.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTarget {
    pub provider: String,
    /// The install source (`owner/repo`), shown for context.
    pub install_ref: String,
    /// The skill's install name — the `skills-lock.json` key passed to update.
    pub name: String,
}

/// Which workspace the install window targets, handed to it on mount. The label
/// lets the window name the project it is installing into. When `update` is set,
/// the window runs in single-skill update mode instead of the install matrix.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallContext {
    pub workspace_id: String,
    pub workspace_label: String,
    /// Present only in update mode: the one skill to update.
    #[serde(default)]
    pub update: Option<UpdateTarget>,
}

/// The context the next-opened install window should read on mount. Set by
/// [`cmd_open_install_window`] before the window loads.
#[derive(Default)]
pub struct InstallContextState(pub Mutex<Option<InstallContext>>);

/// The single in-flight install child, so Cancel can kill it. Installs run
/// sequentially, so one slot is enough.
#[derive(Default)]
pub struct InstallState(pub Mutex<Option<Child>>);

/// Build the install window. Idempotent: returns the existing window if it was
/// already created. Decorated + resizable (unlike the borderless palette).
fn build_install_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(INSTALL_LABEL) {
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, INSTALL_LABEL, WebviewUrl::App("index.html".into()))
        .title("Install Skills")
        .inner_size(INSTALL_WIDTH, INSTALL_HEIGHT)
        .min_inner_size(560.0, 420.0)
        .resizable(true)
        .visible(false)
        .center()
        .build()
}

/// Open (or focus) the install window for `workspace_id`. Stores the context the
/// window reads on mount, then shows + focuses it. If the window was already
/// open, nudges it to re-read the (possibly new) context.
#[tauri::command]
pub async fn cmd_open_install_window(
    app: AppHandle,
    ctx_state: State<'_, InstallContextState>,
    workspace_id: String,
) -> IpcResult<()> {
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;

    let existed = app.get_webview_window(INSTALL_LABEL).is_some();
    {
        let mut guard = ctx_state
            .0
            .lock()
            .map_err(|_| IpcError::new("internal", "install context poisoned"))?;
        *guard = Some(InstallContext {
            workspace_id: target.id,
            workspace_label: target.label,
            update: None,
        });
    }

    let win = build_install_window(&app)
        .map_err(|e| IpcError::new("window_build_failed", e.to_string()))?;
    let _ = win.show();
    let _ = win.set_focus();
    if existed {
        let _ = win.emit("install-context-changed", ());
    }
    Ok(())
}

/// Open (or focus) the install window in **update mode** for one skill. Mirrors
/// [`cmd_open_install_window`] but stashes an [`UpdateTarget`] so the window runs
/// `npx skills update <name>` instead of showing the install matrix. The ref +
/// slug are re-validated in [`cmd_update_skill_stream`] before any spawn.
#[tauri::command]
pub async fn cmd_open_update_window(
    app: AppHandle,
    ctx_state: State<'_, InstallContextState>,
    workspace_id: String,
    provider: String,
    install_ref: String,
    name: String,
) -> IpcResult<()> {
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;

    let existed = app.get_webview_window(INSTALL_LABEL).is_some();
    {
        let mut guard = ctx_state
            .0
            .lock()
            .map_err(|_| IpcError::new("internal", "install context poisoned"))?;
        *guard = Some(InstallContext {
            workspace_id: target.id,
            workspace_label: target.label,
            update: Some(UpdateTarget {
                provider,
                install_ref,
                name,
            }),
        });
    }

    let win = build_install_window(&app)
        .map_err(|e| IpcError::new("window_build_failed", e.to_string()))?;
    let _ = win.show();
    let _ = win.set_focus();
    if existed {
        let _ = win.emit("install-context-changed", ());
    }
    Ok(())
}

/// Read the install context set for this window (cloned so a reopen can re-read
/// it). Errors when nothing is set — the window must be opened via
/// [`cmd_open_install_window`].
#[tauri::command]
pub async fn cmd_take_install_context(
    ctx_state: State<'_, InstallContextState>,
) -> IpcResult<InstallContext> {
    ctx_state
        .0
        .lock()
        .map_err(|_| IpcError::new("internal", "install context poisoned"))?
        .clone()
        .ok_or_else(|| IpcError::new("no_install_context", "No install context set"))
}

/// Stream one line of `pipe` per [`SkillInstallEvent::Line`] until it closes.
fn stream_pipe<R: Read>(pipe: R, stream: &str, ch: &Channel<SkillInstallEvent>) {
    let reader = BufReader::new(pipe);
    for line in reader.lines() {
        match line {
            Ok(text) => {
                let _ = ch.send(SkillInstallEvent::Line {
                    stream: stream.to_string(),
                    text,
                });
            }
            Err(_) => break,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillInput {
    pub provider: String,
    pub install_ref: String,
    pub workspace_id: String,
    /// The one skill slug to install. Pins `--skill` so a multi-skill repo does
    /// not open an interactive picker (which would hang this headless run).
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub tool_ids: Vec<ToolId>,
}

/// Install one skill into the target workspace, streaming output live and
/// terminating with a single [`SkillInstallEvent::Done`]. The child is stored in
/// [`InstallState`] so [`cmd_cancel_install`] can kill it mid-run. On success
/// the watcher is nudged and `workspace-changed` is emitted so the main window
/// re-scans the read-only inventory.
#[tauri::command]
pub async fn cmd_install_skill_stream(
    app: AppHandle,
    install_state: State<'_, InstallState>,
    watcher: State<'_, WatcherState>,
    input: InstallSkillInput,
    on_event: Channel<SkillInstallEvent>,
) -> IpcResult<()> {
    if provider_for(&input.provider).is_none() {
        return Err(IpcError::new(
            "unknown_provider",
            "Unknown skill source provider",
        ));
    }
    if !validate_install_ref(&input.install_ref) {
        return Err(IpcError::from(CoreError::InvalidSkillRef(
            input.install_ref.clone(),
        )));
    }
    if let Some(slug) = &input.slug {
        if !validate_skill_slug(slug) {
            return Err(IpcError::new("invalid_skill_slug", "Unsafe skill slug"));
        }
    }
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == input.workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;
    if !target.dir.is_dir() {
        return Err(IpcError::from(CoreError::NotADirectory(target.dir.clone())));
    }

    // Echo the resolved, non-interactive invocation so the console reads like a
    // terminal. `--skill`/`--agent` are explicit (no picker prompts), so the
    // command line itself shows exactly what runs.
    let slug = input.slug.as_deref();
    let args = skills_npx_args(&input.install_ref, slug, &input.tool_ids);
    let _ = on_event.send(SkillInstallEvent::Line {
        stream: "stdout".into(),
        text: format!("$ npx {}", args.join(" ")),
    });

    let mut cmd = skills_install_command(&input.install_ref, slug, &input.tool_ids);
    cmd.current_dir(&target.dir);
    run_skill_stream(app, install_state, watcher, cmd, on_event).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillInput {
    pub provider: String,
    pub workspace_id: String,
    /// The skill's install name — the `skills-lock.json` key. Re-validated as a
    /// slug before it reaches a process arg.
    pub name: String,
}

/// Update one already-installed skill in the target workspace via
/// `npx skills update <name> --project --yes`, streaming output live and
/// finishing with a single [`SkillInstallEvent::Done`]. Shares the streaming +
/// cancel + re-scan machinery with install (see [`run_skill_stream`]).
#[tauri::command]
pub async fn cmd_update_skill_stream(
    app: AppHandle,
    install_state: State<'_, InstallState>,
    watcher: State<'_, WatcherState>,
    input: UpdateSkillInput,
    on_event: Channel<SkillInstallEvent>,
) -> IpcResult<()> {
    if provider_for(&input.provider).is_none() {
        return Err(IpcError::new(
            "unknown_provider",
            "Unknown skill source provider",
        ));
    }
    if !validate_skill_slug(&input.name) {
        return Err(IpcError::new("invalid_skill_slug", "Unsafe skill name"));
    }
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == input.workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;
    if !target.dir.is_dir() {
        return Err(IpcError::from(CoreError::NotADirectory(target.dir.clone())));
    }

    let args = skills_update_npx_args(&input.name);
    let _ = on_event.send(SkillInstallEvent::Line {
        stream: "stdout".into(),
        text: format!("$ npx {}", args.join(" ")),
    });

    let mut cmd = skills_update_command(&input.name);
    cmd.current_dir(&target.dir);
    run_skill_stream(app, install_state, watcher, cmd, on_event).await
}

/// Spawn `cmd` (cwd already set), stream its stdout/stderr live as
/// [`SkillInstallEvent::Line`] events, store the child in [`InstallState`] so
/// Cancel can kill it, and finish with a single [`SkillInstallEvent::Done`]. On
/// a clean success the watcher is nudged and `workspace-changed` is emitted so
/// the main window re-scans the read-only inventory. Shared by install + update.
async fn run_skill_stream(
    app: AppHandle,
    install_state: State<'_, InstallState>,
    watcher: State<'_, WatcherState>,
    mut cmd: Command,
    on_event: Channel<SkillInstallEvent>,
) -> IpcResult<()> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        // The common failure: `npx` isn't on the resolved PATH. Map it to the
        // actionable hint instead of the opaque "No such file or directory".
        std::io::ErrorKind::NotFound => IpcError::from(CoreError::SkillCliMissing),
        _ => IpcError::new("install_failed", e.to_string()),
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut guard = install_state
            .0
            .lock()
            .map_err(|_| IpcError::new("internal", "install lock poisoned"))?;
        // A prior child (if any) is replaced; runs are sequential, one at a time.
        *guard = Some(child);
    }

    // Stream stdout + stderr concurrently; the blocking reads live off the async
    // workers. Join when both pipes close — the process is exiting by then.
    let ch = on_event.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let mut handles = Vec::new();
        if let Some(out) = stdout {
            let c = ch.clone();
            handles.push(std::thread::spawn(move || stream_pipe(out, "stdout", &c)));
        }
        if let Some(err) = stderr {
            let c = ch.clone();
            handles.push(std::thread::spawn(move || stream_pipe(err, "stderr", &c)));
        }
        for h in handles {
            let _ = h.join();
        }
    })
    .await;

    // Resolve the outcome: a Cancel took the child (→ cancelled); otherwise it is
    // still ours to wait on for the exit status.
    let taken = install_state
        .0
        .lock()
        .map_err(|_| IpcError::new("internal", "install lock poisoned"))?
        .take();
    let (ok, cancelled) = match taken {
        Some(mut child) => (child.wait().map(|s| s.success()).unwrap_or(false), false),
        None => (false, true),
    };

    if ok && !cancelled {
        // The project's tool dirs changed — re-subscribe the watcher and tell
        // the main window to re-scan the read-only inventory.
        watcher.restart_if_running(app.clone());
        let _ = app.emit("workspace-changed", ());
    }
    let _ = on_event.send(SkillInstallEvent::Done { ok, cancelled });
    Ok(())
}

/// Kill the in-flight install child, if any. The stream loop then sees closed
/// pipes and the slot is empty, so the run reports `cancelled`.
#[tauri::command]
pub async fn cmd_cancel_install(install_state: State<'_, InstallState>) -> IpcResult<()> {
    kill_active(&install_state);
    Ok(())
}

/// Kill the stored install child (best-effort). Shared by the Cancel command and
/// the install-window close handler so an abandoned install never lingers.
fn kill_active(install_state: &InstallState) {
    if let Ok(mut guard) = install_state.0.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
        }
    }
}

/// Close-handler hook: kill any in-flight child when the install window closes,
/// so abandoning the window never strands a running `npx`.
pub fn on_install_window_closed(app: &AppHandle) {
    kill_active(&app.state::<InstallState>());
}
