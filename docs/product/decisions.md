# Decision log

> **Role of this doc.** The product's memory: why the big calls were made, so we
> don't relitigate settled questions or accidentally undo load-bearing ones.
> Lightweight ADR format. **Made** decisions are immutable history (supersede,
> don't edit); **open** decisions are tracked until resolved.

**Status:** ✅ in force · ⛔ superseded · ❓ open

---

## Made

### D1 — Filesystem is the source of truth ✅
**Context:** every other tool reads disk; a shadow DB would drift.
**Decision:** no database. Scan disk, plan from disk, apply to disk. State is
always re-derivable. **Consequence:** correctness over speed; manual/triggered
rescans, no background cache to invalidate.

### D2 — Plan-then-apply, partial-tolerant ✅
**Decision:** stage in memory → plan from current disk → apply explicitly. One
failing op never aborts the rest; results report per-op counts + reasons.
**Why it matters:** the user always sees what will happen before it happens.

### D3 — Never overwrite real files; conflicts surface as skips ✅
**Decision:** a real file/dir or basename collision becomes `skip_conflict`,
never a silent clobber. Foreign files are taken over only on **explicit**
per-row confirmation (`0.1.1`), never by watcher/suite/workspace flows.

### D4 — Suite storage stays `~/.agentic-suites.json` ✅
**Context:** migrating users come from the VS Code extension.
**Decision:** keep the path verbatim for cross-app compatibility (resolved PRD
open question). Not moved under `~/.agentic-hub/`.

### D5 — Managed-block markers are verbatim ✅
**Decision:** `<!-- agentic-hub:start -->` / `:end` under `## Agentic Hub
Managed Rules`. Identical to the rebranded VS Code extension so instruction
files migrate untouched. Legacy `e-studio-*` names are never reintroduced.

### D6 — No `tauri-plugin-shell`, ever ✅
**Decision:** the WebView never gets shell execution. A login shell is used
**only** to read `PATH`; any subprocess runs via `std::process::Command` with a
fixed, validated arg vector. Adding the shell plugin requires a security review.

### D7 — Workspace scope is read-only ✅ (supersedes PRD M3 write flow)
**Date:** 2026-06-04 · **Ship:** `0.5.0`
**Context:** the PRD's M3 hard-copied a suite into a project (`WorkspacePatchService`,
manifest cycle, out-of-workspace guard). It added a second write surface and a
manifest to keep consistent.
**Decision:** remove it. Workspace scope **scans and reports** what each tool
already has; it never writes. Capabilities are written only by the global
projection engine. **Removed:** `cmd_apply_workspace_patch`, `workspace_patch`
module, `WorkspacePatchResult`/`WorkspaceApply` types, `WorkspaceTarget.lastApplied`,
the `workspace-patch.json` manifest, `workspace-apply-progress` event.

### D8 — One opt-in workspace write: skill install ✅
**Ship:** `0.6.0`
**Context:** D7 made workspace read-only, but installing a starred skill into a
project is a genuine user need.
**Decision:** the **single** workspace write path is `cmd_install_skill` —
explicit, user-initiated, never automatic, never part of scanning. It runs the
source CLI via a controlled subprocess (validated `owner/repo`, cwd = remembered
workspace), keeps no install state, and re-scans the read-only inventory after.
Consistent with D6 (still no shell plugin).

### D9 — Command palette is a dedicated floating window ✅
**Ship:** `0.3.0`
**Decision:** an `NSPanel`-style always-on-top window (label `palette`), not an
in-app overlay, so it works while the Hub is backgrounded. Adds
`tauri-plugin-global-shortcut` and a persisted `Settings.paletteShortcut`.
The install surface later reused the same dedicated-window pattern (`0.6.1`).

### D10 — Suite refs are source-qualified for portability ✅
**Ship:** `0.4.0`
**Decision:** every `CapabilityItem` carries a portable `SourceRef`; suite
entries are `SuiteCapabilityRef { cap, source }`. A synced suite resolves
per-source — a ref whose source is absent on this machine is **skipped and
preserved**, never deleted, never mis-resolved onto a same-named capability from
a different source. This is our answer to "sync" without a cloud (see backlog L-non-goals).

### D11 — Base suite + suite↔tool bindings ✅
**Ship:** `0.4.0`
**Decision:** a suite can be marked **base** (≤1, store-enforced); its caps union
into every global apply. A persisted `suite-bindings.json` records which suite is
applied to each tool; editing a suite re-applies (full reset) to every bound tool
(serialized via the reconcile guard). Deleting a suite drops bindings without
wiping tool projections.

### D12 — Unified Global/Workspace scope rail ✅
**Ship:** `0.6.1`
**Decision:** retire the header scope `Select`; render one left rail with Global
pinned on top and remembered workspaces below. Switching scope kind resets
scope-specific filters so a `source` chosen in one scope can't blank the matrix
in the other.

### D13 — skills.sh search runs through Rust (keyless index) ✅
**Ship:** `0.6.0`
**Context:** WKWebView enforces CORS and the public index sends no CORS header.
**Decision:** search via a blocking Rust `reqwest` GET to the keyless public
`skills.sh/api/search` (same endpoint the CLI uses) — no API key required, no
WebView `fetch`. "Star" is **local-only** (no remote favorites endpoint).

### D14 — `RELEASE.md` holds only the current release ✅
**Decision:** the release workflow publishes `RELEASE.md` verbatim as the GitHub
Release body. Cutting a release **replaces** it (never prepends); cumulative
history lives in `CHANGELOG.md`, newest first.

### D15 — Opt-in Aptabase telemetry ✅ (resolves D-open-3)
**Date:** 2026-06-15
**Context:** D-open-3 left telemetry open, to be revisited only if a concrete
optimization question needed real usage data. We chose to add the lightest
honest signal — does the app get launched — without compromising the
local-first posture.
**Decision:** add **opt-in** anonymous telemetry via `tauri-plugin-aptabase`,
**off by default**, toggled in Config (`Settings.telemetry.enabled`, mirrored at
runtime by a `TelemetryState` flag so a toggle takes effect with no restart).
Only coarse lifecycle events (`app_started`, `app_exited`) are sent, **from Rust
only** — the WebView never calls out, so there is no `@aptabase/tauri` binding
and no `aptabase:allow-track-event` ACL entry. Nothing is sent while disabled.
**Consequence:** the v1 "no remote network calls" stance now has one
user-consented exception. Evaluation lives in
[telemetry-options.md](telemetry-options.md).

---

## Open

### D-open-1 — Windows: ship constrained or skip? ❓
Symlinks need Developer Mode or admin elevation on Windows. The `managed_copy`
path technically works but is unvalidated. **Decide before any Windows release.**
Tracked as backlog L1.

### D-open-2 — Auto-update hosting ❓
`tauri-plugin-updater` needs hosted signed manifests. v1 is unsigned/unhosted on
non-macOS. **Decide hosting + signing before public multi-OS release.** Backlog X5.

### D-open-3 — Telemetry ✅ resolved by D15
Resolved: opt-in Aptabase telemetry, off by default, Rust-only lifecycle events.
See D15. Backlog L3 done.

### D-open-4 — API-key storage ❓
Plaintext in `config.json` today (accepted v1 tradeoff, tech-debt T1). Move to OS
keychain (backlog X6)? Decide if/when a second authed source lands.

### D-open-5 — OpenClaw workspace support ❓
Out of scope today (matches the VS Code decision). Revisit when OpenClaw's
project-level scan path stabilizes. Backlog L2.

---

## Adding a decision

One entry per decision: **Context → Decision → Consequence**, dated, with the
shipping version when known. Never edit a Made decision to reverse it — add a new
one and mark the old ⛔ superseded (as D7 did to the PRD's M3 write flow).
