//! Dedicated, live-streaming skill-install window + its commands.
//!
//! The workspace FAB opens a real `install` window (not a modal) that loads the
//! starred-skill matrix, runs `npx skills add` per selected skill while
//! streaming stdout/stderr live over a Tauri [`Channel`], and exposes a Cancel
//! that kills the in-flight child. The window has two scopes ([`InstallScope`]):
//! **Workspace** (the original path — installs into a project via the
//! workspace store; the inventory scan itself stays read-only) and **Library**
//! (installs into a Hub source root's contract layout, so one install is
//! projectable across every tool and project). Either way the target dir is
//! resolved server-side, never an arbitrary path, and the argv is a fixed,
//! validated vector — see `docs/tech/modules/skill-sources.md`.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use agentic_core::error::CoreError;
use agentic_core::model::{ToolId, WorkspaceTarget};
use agentic_core::settings::{Settings, SourceConfig};
use agentic_core::skill_source::{
    locate_installed_skill, provider_for, skills_install_command,
    skills_library_install_command, skills_library_npx_args, skills_npx_args,
    skills_update_command, skills_update_npx_args, validate_install_ref, validate_skill_slug,
    SkillInstallEvent,
};
use agentic_core::source_skill_lock::{
    normalize_into_source_root, read_source_lock, upsert_entry, validate_dest_subpath,
    write_source_lock, SourceLockEntry,
};
use agentic_core::workspace_target_store::WorkspaceTargetStore;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, Position, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::error::IpcError;
use crate::watcher::WatcherState;

type IpcResult<T> = Result<T, IpcError>;

/// Label of the install window.
pub const INSTALL_LABEL: &str = "install";

const INSTALL_WIDTH: f64 = 760.0;
const INSTALL_HEIGHT: f64 = 620.0;

/// Where a skill install/update writes: a workspace project (the original
/// path) or a Hub source root — the shared agentic-resources library, so one
/// install is projectable across every tool and project. See
/// `docs/tech/modules/skill-sources.md#library-install`.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallScope {
    #[default]
    Workspace,
    Library,
}

/// A single skill the window should **update** (rather than install). Set when
/// the window is opened from a row's "Update via skills.sh" action; the window
/// then re-runs the install for this one skill instead of showing the install
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
    /// The skill's install name — the lock key passed to update.
    pub name: String,
    #[serde(default)]
    pub scope: InstallScope,
    /// Library scope only: which source root to re-install into.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Library scope only: destination subpath under `skills/`, recorded at
    /// install time.
    #[serde(default)]
    pub dest_subpath: Option<String>,
    /// Library scope only: the `--skill` slug to reinstall, when it differs
    /// from `name`.
    #[serde(default)]
    pub slug: Option<String>,
}

/// Which workspace the install window targets, handed to it on mount. The label
/// lets the window name the project it is installing into. `workspace_id` /
/// `workspace_label` are absent when the window is opened directly in Library
/// scope (no workspace involved). When `update` is set, the window runs in
/// single-skill update mode instead of the install matrix.
#[cfg_attr(
    feature = "ts-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../src/types/generated/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallContext {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub workspace_label: Option<String>,
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
/// Parented to the main window so macOS keeps it on the same Space/desktop.
fn build_install_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(INSTALL_LABEL) {
        return Ok(win);
    }
    let mut builder =
        WebviewWindowBuilder::new(app, INSTALL_LABEL, WebviewUrl::App("index.html".into()))
            .title("Install Skills")
            .inner_size(INSTALL_WIDTH, INSTALL_HEIGHT)
            .min_inner_size(560.0, 420.0)
            .resizable(true)
            .visible(false);
    if let Some(main) = app.get_webview_window("main") {
        builder = builder.parent(&main)?;
    }
    let window = builder.build()?;
    let theme = window.theme().unwrap_or(tauri::Theme::Light);
    crate::appearance::sync_webview_background(&window, theme);
    Ok(window)
}

/// Show and focus the install window, centered over the main window when possible.
fn show_install_window(app: &AppHandle, win: &WebviewWindow) {
    center_over_main(app, win);
    let _ = win.show();
    let _ = win.set_focus();
}

/// Place `win` centered over the main window in logical coordinates so it lands
/// on the same monitor and Space as the hub.
fn center_over_main(app: &AppHandle, win: &WebviewWindow) {
    let Some(main) = app.get_webview_window("main") else {
        let _ = win.center();
        return;
    };
    let Ok(main_pos) = main.outer_position() else {
        let _ = win.center();
        return;
    };
    let Ok(main_size) = main.outer_size() else {
        let _ = win.center();
        return;
    };
    let Ok(inst_size) = win.outer_size() else {
        let _ = win.center();
        return;
    };
    let scale = main.scale_factor().unwrap_or(1.0);
    let x = main_pos.x as f64 / scale
        + (main_size.width as f64 / scale - inst_size.width as f64 / scale) / 2.0;
    let y = main_pos.y as f64 / scale
        + (main_size.height as f64 / scale - inst_size.height as f64 / scale) / 2.0;
    let _ = win.set_position(Position::Logical(LogicalPosition { x, y }));
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
            workspace_id: Some(target.id),
            workspace_label: Some(target.label),
            update: None,
        });
    }

    let win = build_install_window(&app)
        .map_err(|e| IpcError::new("window_build_failed", e.to_string()))?;
    show_install_window(&app, &win);
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
            workspace_id: Some(target.id),
            workspace_label: Some(target.label),
            update: Some(UpdateTarget {
                provider,
                install_ref,
                name,
                scope: InstallScope::Workspace,
                source_id: None,
                dest_subpath: None,
                slug: None,
            }),
        });
    }

    let win = build_install_window(&app)
        .map_err(|e| IpcError::new("window_build_failed", e.to_string()))?;
    show_install_window(&app, &win);
    if existed {
        let _ = win.emit("install-context-changed", ());
    }
    Ok(())
}

/// Open (or focus) the install window in **update mode** for one Library-scope
/// skill — the Global Manager's "Update via skills.sh" action for a row a
/// source root's lock manages. No workspace is involved: `source_id` names the
/// source root to re-install into and `dest_subpath` is the destination the
/// lock recorded at install time. The ref + slug are re-validated in
/// [`cmd_update_skill_stream`] before any spawn.
#[tauri::command]
pub async fn cmd_open_library_update_window(
    app: AppHandle,
    ctx_state: State<'_, InstallContextState>,
    provider: String,
    install_ref: String,
    name: String,
    source_id: String,
    dest_subpath: String,
) -> IpcResult<()> {
    let existed = app.get_webview_window(INSTALL_LABEL).is_some();
    {
        let mut guard = ctx_state
            .0
            .lock()
            .map_err(|_| IpcError::new("internal", "install context poisoned"))?;
        *guard = Some(InstallContext {
            workspace_id: None,
            workspace_label: None,
            update: Some(UpdateTarget {
                provider,
                install_ref,
                name,
                scope: InstallScope::Library,
                source_id: Some(source_id),
                dest_subpath: Some(dest_subpath),
                slug: None,
            }),
        });
    }

    let win = build_install_window(&app)
        .map_err(|e| IpcError::new("window_build_failed", e.to_string()))?;
    show_install_window(&app, &win);
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
    #[serde(default)]
    pub scope: InstallScope,
    /// Workspace scope only.
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Library scope only: which source root to install into.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Library scope only: destination subpath under `skills/` (empty lands
    /// the skill directly at `skills/<name>/`).
    #[serde(default)]
    pub dest_subpath: Option<String>,
    /// The one skill slug to install. Pins `--skill` so a multi-skill repo does
    /// not open an interactive picker (which would hang this headless run).
    /// Required for Library scope (the lock needs the exact install name).
    #[serde(default)]
    pub slug: Option<String>,
    /// Workspace scope only.
    #[serde(default)]
    pub tool_ids: Vec<ToolId>,
}

/// Resolve and validate a workspace target by id, erroring with the same
/// `workspace_not_found` / `NotADirectory` shape both install and update use.
fn resolve_workspace_target(workspace_id: Option<&str>) -> IpcResult<WorkspaceTarget> {
    let workspace_id =
        workspace_id.ok_or_else(|| IpcError::new("workspace_not_found", "No workspace selected"))?;
    let target = WorkspaceTargetStore::new()
        .read()?
        .workspace_targets
        .into_iter()
        .find(|t| t.id == workspace_id)
        .ok_or_else(|| IpcError::new("workspace_not_found", "Workspace target no longer exists"))?;
    if !target.dir.is_dir() {
        return Err(IpcError::from(CoreError::NotADirectory(target.dir)));
    }
    Ok(target)
}

/// Resolve and validate a Hub source root by id, for the Library scope.
fn resolve_source(source_id: Option<&str>) -> IpcResult<SourceConfig> {
    let source_id =
        source_id.ok_or_else(|| IpcError::new("source_not_found", "No source selected"))?;
    let source = Settings::load()?
        .resolve_sources()
        .into_iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| IpcError::new("source_not_found", "Source no longer exists"))?;
    if !source.path.is_dir() {
        return Err(IpcError::from(CoreError::NotADirectory(source.path)));
    }
    Ok(source)
}

/// Echo the resolved, non-interactive invocation so the console reads like a
/// terminal — `--skill`/`--agent` are explicit (no picker prompts), so the
/// command line itself shows exactly what runs.
fn echo_command(on_event: &Channel<SkillInstallEvent>, program: &str, args: &[String]) {
    let _ = on_event.send(SkillInstallEvent::Line {
        stream: "stdout".into(),
        text: format!("$ {program} {}", args.join(" ")),
    });
}

/// Everything [`finish_library`] needs to normalize a staged install into the
/// contract layout and record it in the source-root lock, once the streamed
/// process has finished. `staging` is dropped (and its directory removed) at
/// the end of `finish_library` regardless of outcome.
struct LibraryInstallCtx {
    staging: tempfile::TempDir,
    source: SourceConfig,
    dest_subpath: String,
    name: String,
    install_ref: String,
}

/// After a library-scope install/update process exits, locate the skill it
/// staged, copy it into `<source>/skills/<destSubpath>/<name>/`, and upsert the
/// source-root lock. Streams a diagnostic line and returns `false` on any
/// failure past the process exit itself (nothing landed, or a partial normalize
/// failed) — the caller reports this as a failed run. Emits `sources-changed` on
/// success so the Global Manager re-scans. `ctx.staging` is removed when this
/// function returns, on every path.
fn finish_library(
    app: &AppHandle,
    ctx: LibraryInstallCtx,
    ok: bool,
    cancelled: bool,
    on_event: &Channel<SkillInstallEvent>,
) -> bool {
    if !ok || cancelled {
        return false;
    }
    let Some(found) = locate_installed_skill(ctx.staging.path(), &ctx.name) else {
        let _ = on_event.send(SkillInstallEvent::Line {
            stream: "stderr".into(),
            text: "Could not locate the installed skill folder after install.".into(),
        });
        return false;
    };
    let installed =
        match normalize_into_source_root(&found, &ctx.source.path, &ctx.dest_subpath, &ctx.name) {
            Ok(i) => i,
            Err(e) => {
                let _ = on_event.send(SkillInstallEvent::Line {
                    stream: "stderr".into(),
                    text: e.to_string(),
                });
                return false;
            }
        };
    let mut lock = match read_source_lock(&ctx.source.path) {
        Ok(l) => l,
        Err(e) => {
            let _ = on_event.send(SkillInstallEvent::Line {
                stream: "stderr".into(),
                text: e.to_string(),
            });
            return false;
        }
    };
    upsert_entry(
        &mut lock,
        &installed.name,
        SourceLockEntry {
            source: ctx.install_ref,
            source_type: "github".to_string(),
            skill_path: installed.skill_path,
            computed_hash: installed.computed_hash,
        },
    );
    if let Err(e) = write_source_lock(&ctx.source.path, &lock) {
        let _ = on_event.send(SkillInstallEvent::Line {
            stream: "stderr".into(),
            text: e.to_string(),
        });
        return false;
    }
    // The source root changed — tell the main window to re-scan the Global
    // Manager. Unlike workspace scope, there is no "active workspace" tool
    // dir to re-subscribe the watcher to; the source root is already watched
    // whenever the watcher is running.
    let _ = app.emit("sources-changed", ());
    true
}

/// Install one skill, streaming output live and terminating with a single
/// [`SkillInstallEvent::Done`]. The child is stored in [`InstallState`] so
/// [`cmd_cancel_install`] can kill it mid-run.
///
/// Workspace scope runs in the target project dir; on success the watcher is
/// nudged and `workspace-changed` is emitted. Library scope stages the install
/// in a scratch dir, then normalizes the produced skill into a Hub source
/// root's contract layout and records it in that root's `skills-lock.json`;
/// on success `sources-changed` is emitted.
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

    match input.scope {
        InstallScope::Workspace => {
            let target = resolve_workspace_target(input.workspace_id.as_deref())?;
            let slug = input.slug.as_deref();
            let args = skills_npx_args(&input.install_ref, slug, &input.tool_ids);
            echo_command(&on_event, "npx", &args);

            let mut cmd = skills_install_command(&input.install_ref, slug, &input.tool_ids);
            cmd.current_dir(&target.dir);
            let (ok, cancelled) = run_skill_stream(&install_state, cmd, &on_event).await?;
            if ok && !cancelled {
                watcher.restart_if_running(app.clone());
                let _ = app.emit("workspace-changed", ());
            }
            let _ = on_event.send(SkillInstallEvent::Done { ok, cancelled });
            Ok(())
        }
        InstallScope::Library => {
            let dest_subpath = input.dest_subpath.clone().unwrap_or_default();
            if !validate_dest_subpath(&dest_subpath) {
                return Err(IpcError::new(
                    "invalid_dest_subpath",
                    "Unsafe destination path",
                ));
            }
            let source = resolve_source(input.source_id.as_deref())?;
            let slug = input
                .slug
                .clone()
                .ok_or_else(|| IpcError::new("invalid_skill_slug", "Library install requires a skill"))?;

            let staging = tempfile::tempdir()
                .map_err(|e| IpcError::new("staging_failed", e.to_string()))?;
            let args = skills_library_npx_args(&input.install_ref, Some(&slug));
            echo_command(&on_event, "npx", &args);

            let mut cmd = skills_library_install_command(&input.install_ref, Some(&slug));
            cmd.current_dir(staging.path());
            let (ok, cancelled) = run_skill_stream(&install_state, cmd, &on_event).await?;
            let ctx = LibraryInstallCtx {
                staging,
                source,
                dest_subpath,
                name: slug,
                install_ref: input.install_ref.clone(),
            };
            let ok = finish_library(&app, ctx, ok, cancelled, &on_event);
            let _ = on_event.send(SkillInstallEvent::Done { ok, cancelled });
            Ok(())
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillInput {
    pub provider: String,
    #[serde(default)]
    pub scope: InstallScope,
    /// Workspace scope only.
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// The skill's install name — the lock key. Re-validated as a slug before
    /// it reaches a process arg.
    pub name: String,
    /// Library scope only: which source root to re-install into.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Library scope only: the ref to re-install (recorded at install time).
    #[serde(default)]
    pub install_ref: Option<String>,
    /// Library scope only: `--skill` slug, when it differs from `name`.
    #[serde(default)]
    pub slug: Option<String>,
    /// Library scope only: destination subpath under `skills/`, recorded at
    /// install time.
    #[serde(default)]
    pub dest_subpath: Option<String>,
}

/// Update one already-installed skill, streaming output live and finishing
/// with a single [`SkillInstallEvent::Done`]. Shares the streaming + cancel
/// machinery with install (see [`run_skill_stream`]).
///
/// Workspace scope delegates to `npx skills update <name> --project --yes`.
/// Library scope has no such CLI counterpart once a skill has been normalized
/// into the contract layout (it may have moved/nested under `skills/`), so it
/// **re-runs the staged install** for the recorded ref/slug and overwrites the
/// same destination — the Hub-owned equivalent of an update.
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

    match input.scope {
        InstallScope::Workspace => {
            let target = resolve_workspace_target(input.workspace_id.as_deref())?;
            let args = skills_update_npx_args(&input.name);
            echo_command(&on_event, "npx", &args);

            let mut cmd = skills_update_command(&input.name);
            cmd.current_dir(&target.dir);
            let (ok, cancelled) = run_skill_stream(&install_state, cmd, &on_event).await?;
            if ok && !cancelled {
                watcher.restart_if_running(app.clone());
                let _ = app.emit("workspace-changed", ());
            }
            let _ = on_event.send(SkillInstallEvent::Done { ok, cancelled });
            Ok(())
        }
        InstallScope::Library => {
            let source = resolve_source(input.source_id.as_deref())?;
            let install_ref = input.install_ref.clone().ok_or_else(|| {
                IpcError::new("invalid_skill_ref", "Missing install ref for library update")
            })?;
            if !validate_install_ref(&install_ref) {
                return Err(IpcError::from(CoreError::InvalidSkillRef(install_ref)));
            }
            let slug = input.slug.clone().unwrap_or_else(|| input.name.clone());
            if !validate_skill_slug(&slug) {
                return Err(IpcError::new("invalid_skill_slug", "Unsafe skill slug"));
            }
            let dest_subpath = input.dest_subpath.clone().unwrap_or_default();
            if !validate_dest_subpath(&dest_subpath) {
                return Err(IpcError::new(
                    "invalid_dest_subpath",
                    "Unsafe destination path",
                ));
            }

            let staging = tempfile::tempdir()
                .map_err(|e| IpcError::new("staging_failed", e.to_string()))?;
            let args = skills_library_npx_args(&install_ref, Some(&slug));
            echo_command(&on_event, "npx", &args);

            let mut cmd = skills_library_install_command(&install_ref, Some(&slug));
            cmd.current_dir(staging.path());
            let (ok, cancelled) = run_skill_stream(&install_state, cmd, &on_event).await?;
            let ctx = LibraryInstallCtx {
                staging,
                source,
                dest_subpath,
                name: input.name.clone(),
                install_ref,
            };
            let ok = finish_library(&app, ctx, ok, cancelled, &on_event);
            let _ = on_event.send(SkillInstallEvent::Done { ok, cancelled });
            Ok(())
        }
    }
}

/// Spawn `cmd` (cwd already set), stream its stdout/stderr live as
/// [`SkillInstallEvent::Line`] events, and store the child in [`InstallState`]
/// so Cancel can kill it. Returns `(ok, cancelled)` once the process exits (or
/// is killed) — callers own the scope-specific post-processing and the
/// terminal [`SkillInstallEvent::Done`]. Shared by install + update, both scopes.
async fn run_skill_stream(
    install_state: &State<'_, InstallState>,
    mut cmd: Command,
    on_event: &Channel<SkillInstallEvent>,
) -> IpcResult<(bool, bool)> {
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
    Ok((ok, cancelled))
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
