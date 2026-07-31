# CLI tools preflight

The **Tools** pane in the Resources panel is a *preflight*: before building
agentic systems, the user confirms the command-line tools their agents rely on
(Node, Python, Homebrew, git, ripgrep, the GitHub/GitLab CLIs, the agent CLIs, Vercel)
are installed and — where it matters — authenticated. It is read-only and
advisory: the hub never installs or signs anything in; it probes and reports,
and links out to each tool's install page.

This sits beside the skills.sh browser ([skill-sources.md](skill-sources.md))
and session explorer ([session-explorer.md](session-explorer.md)) in the
Resources rail. The rail is ordered **Skills → Tools → Sessions**; Skills is
the default only when the skills.sh source is enabled in Config, while Tools is
always available as the default and fallback.

## Data: a catalog, not code

Tools are **data**. The bundled catalog ships at
[`resources/cli-tools/catalog.json`](../../../resources/cli-tools/catalog.json)
and is embedded into the binary via `include_str!` (no network, no missing-file
failure). Each entry declares how to identify, check, install, and optionally
auth-check one tool:

```jsonc
{
  "id": "gh",                       // stable id; addresses a row, merges overrides
  "name": "GitHub CLI",
  "category": "Version control",
  "installUrl": "https://cli.github.com",
  "check": { "program": "gh", "args": ["--version"] },   // succeeds ⇒ installed
  "auth":  { "program": "gh", "args": ["auth", "status"] } // succeeds ⇒ authed
}
```

`auth` is declared only where a tool has a stable status subcommand
(`gh auth status`, `glab auth status`, `codex login status`,
`cursor-agent status`, `vercel whoami`). Tools without it (Node, Python,
Homebrew, git, ripgrep, Claude Code) simply omit `auth` and report `notApplicable`.

### User override (and a deferred remote source)

`settings.cliToolsPath` (`#[serde(default)]`, so legacy configs still load)
points at an optional user-local catalog JSON. It is **merged over** the bundled
set by `cli_tools::merge_catalogs`: a custom entry with the same `id` replaces
the bundled one in place (order preserved); a new id is appended. A missing
override file falls back to the bundled set; a malformed one surfaces a typed
error. A remote/hot-update source is intentionally deferred — the merge seam is
already in place for it.

## Probe: Rust runs the check, with the login PATH and a timeout

`cli_tools::check_tool(&CliTool) -> CliToolStatus` is the one impure entry point:

1. Run `check`. Success ⇒ `installed = true`; the version is the first non-empty
   stdout line (`parse_version`, kept pure and unit-tested — tools format
   versions wildly, so the raw first line is surfaced rather than guessed).
2. If installed and `auth` is declared, run it: success ⇒ `authed`, non-zero ⇒
   `notAuthed`, timeout/spawn-failure ⇒ `unknown`.
3. Not installed (spawn failure or non-zero check) ⇒ `installed = false`, auth
   `notApplicable`.

Every spawn uses the resolved **login PATH** (see below) and a hard
`PROBE_TIMEOUT` (8s). The timeout matters because auth checks can touch the
network to validate a token; a hung one must never stall the blocking pool.

### Shared login-PATH resolution

GUI apps launched from the macOS Dock inherit a minimal `PATH` that omits
Homebrew / nvm / fnm, so `gh`, `vercel`, etc. are often invisible. The fix —
asking the user's own shell, as an interactive login shell (`-ilc`), to print
its `PATH` framed by a sentinel so rc-file chatter can't corrupt it — lives in
one place: [`crate::shell_env`](../../../crates/agentic-core/src/shell_env.rs)
(`login_path()` / `command_with_login_path()`). The skills installer
(`skill_source::npx_command`) uses the same helper, per the AGENTS.md "one
place" rule. Resolution is **cached once** (`OnceLock`) and runs under its own
`RESOLVE_TIMEOUT` (5s, kill-on-timeout) so a hanging rc file can't wedge the
app before a probe's own timeout even starts.

## IPC

Two thin window-local commands (registered in `agentic-hub` `lib.rs`):

- `cmd_list_tool_catalog() -> Vec<CliTool>` — bundled + optional override, merged.
- `cmd_check_tool({ id }) -> CliToolStatus` — find the tool by id, probe it on
  the blocking pool (`spawn_blocking`).

Shared types are ts-rs-exported (`CliTool`, `CliCommand`, `AuthState`,
`CliToolStatus`) and consumed by `src/ipc.ts` → `src/state/cliTools.ts`.

## Frontend

`useCliToolsStore` holds the `catalog`, a `statusById` map, and a `checking`
set. On first open the Tools pane loads the catalog, then `checkAll()` probes
every tool in parallel (`Promise.all`) so each row fills in as it resolves; a
slow auth check never blocks the rest. Re-checks are explicit: a per-row
**Re-check** action or a **Refresh all** header button. When a tool is not
installed, the row shows an **Install** action that opens its `installUrl` via
`openUrl` (anchor navigation is a no-op inside the WebView).

## Security

Checks run `Command::new(program).args(args)` with **no shell** (never `sh -c`),
so catalog args can't smuggle metacharacters. The bundled catalog is trusted;
the user-local override runs the user's own `program` + `args` on their own
machine — the same trust boundary as the existing custom source paths — guarded
by the per-probe timeout. No `tauri-plugin-shell` is added; a login shell is
used only to *read* `PATH`, never to run a probe.

Because the override path feeds command execution, it is **config-file-only**:
`cli_tools_path` is set by hand-editing the settings file, never by the
(untrusted) WebView. `cmd_save_settings` preserves the on-disk value and
discards any `cli_tools_path` the renderer sends, so an XSS-compromised UI can't
point the catalog at attacker-chosen executables.
