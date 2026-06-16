//! Login-shell `PATH` resolution shared by every code path that shells out.
//!
//! GUI apps launched from the macOS Dock inherit a minimal `PATH` that omits
//! Homebrew / nvm / fnm, so binaries like `npx`, `gh`, or `vercel` are often
//! invisible. We ask the user's own shell — as an **interactive login** shell —
//! to print its `PATH`, framed by a sentinel so rc-file chatter can't corrupt
//! it. Centralized here (the one place that knows this trick) so both the skill
//! installer and the CLI-tool preflight resolve `PATH` identically.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Sentinel framing our `PATH` print so we can recover it even when a shell's
/// startup files write their own banner/chatter to stdout.
const PATH_MARKER: &str = "__AGENTIC_HUB_PATH__";

/// Upper bound on the login-shell `PATH` probe. A user's rc files can hang
/// (slow network call, prompt waiting on input); without this the probe — and
/// every caller that shells out — would block indefinitely.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Process-wide cache of the resolved login `PATH`. Resolving spawns an
/// interactive login shell, which is expensive; the value is stable for the
/// app's lifetime, so we pay for it once.
static CACHED_LOGIN_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Pull the value framed by `marker` on both sides out of `raw`. Returns `None`
/// when the framing is absent. Pure so the parsing is unit-tested without a
/// shell.
fn extract_framed(raw: &str, marker: &str) -> Option<String> {
    let start = raw.find(marker)? + marker.len();
    let rest = &raw[start..];
    let end = rest.find(marker)?;
    Some(rest[..end].to_string())
}

/// Candidate shells to read `PATH` from, the user's own `$SHELL` first.
fn path_shells() -> Vec<String> {
    let mut shells: Vec<String> = Vec::new();
    if let Ok(s) = std::env::var("SHELL") {
        if !s.is_empty() {
            shells.push(s);
        }
    }
    for fallback in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
        if !shells.iter().any(|s| s == fallback) {
            shells.push(fallback.to_string());
        }
    }
    shells
}

/// Best-effort `PATH` from the user's shell, computed once and cached. We ask
/// the user's shell — as an **interactive login** shell (`-ilc`) so it sources
/// `.zshrc` / `.bashrc`, where version managers and Homebrew almost always put
/// their `PATH` (a plain login shell skips those) — to print its `PATH`, framed
/// by a sentinel so rc-file chatter can't corrupt it. `None` falls back to the
/// inherited environment.
pub fn login_path() -> Option<String> {
    CACHED_LOGIN_PATH.get_or_init(resolve_login_path).clone()
}

/// One-shot resolution behind the [`CACHED_LOGIN_PATH`] cache. Tries each
/// candidate shell with a hard timeout so a hanging rc file can't wedge the app.
fn resolve_login_path() -> Option<String> {
    let script = format!("printf '{PATH_MARKER}%s{PATH_MARKER}' \"$PATH\"");
    for shell in path_shells() {
        if !Path::new(&shell).exists() {
            continue;
        }
        if let Some(raw) = capture_with_timeout(&shell, &script) {
            if let Some(path) = extract_framed(&raw, PATH_MARKER) {
                if !path.trim().is_empty() {
                    return Some(path.trim().to_string());
                }
            }
        }
    }
    None
}

/// Spawn `shell -ilc script`, returning its stdout, or `None` if it can't be
/// spawned or runs past [`RESOLVE_TIMEOUT`] (the child is then killed). stderr
/// is discarded; we only need the framed `PATH` on stdout.
fn capture_with_timeout(shell: &str, script: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-ilc", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let mut out = String::new();
                if let Some(mut handle) = child.stdout.take() {
                    let _ = handle.read_to_string(&mut out);
                }
                return Some(out);
            }
            Ok(None) => {
                if start.elapsed() >= RESOLVE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

/// Build a [`Command`] for `program` with the resolved login `PATH` applied (a
/// no-op when it can't be resolved, falling back to the inherited environment).
/// Callers add their own args / cwd / stdio.
pub fn command_with_login_path(program: &str) -> Command {
    let mut cmd = Command::new(program);
    if let Some(path) = login_path() {
        cmd.env("PATH", path);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_framed_recovers_path_amid_chatter() {
        // rc files can print a banner before/after our framed PATH.
        let raw = "welcome!\n__M__/opt/homebrew/bin:/usr/bin__M__\nbye";
        assert_eq!(
            extract_framed(raw, "__M__").as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(extract_framed("no markers here", "__M__"), None);
        assert_eq!(extract_framed("__M__only-one-side", "__M__"), None);
    }

    #[test]
    fn path_shells_prefers_user_shell_then_falls_back() {
        // Pure ordering check; uses whatever $SHELL the test env carries.
        let shells = path_shells();
        assert!(shells.iter().any(|s| s == "/bin/sh"));
        // No duplicate fallbacks even if $SHELL is one of them.
        let bash = shells.iter().filter(|s| *s == "/bin/bash").count();
        assert!(bash <= 1);
    }
}
