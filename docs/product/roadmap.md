# Roadmap

> **Role of this doc.** This is the *living* milestone tracker. [PRODUCT.md](../../PRODUCT.md)
> is the static PRD (what we set out to build on 2026-05-20); this roadmap records
> what actually shipped and where we go next. When the two disagree, this doc is
> the current truth and PRODUCT.md is the original intent.

**Mission anchor:** help people manage their agentic resources — skills, agents,
rules, hooks — across every AI tool from one place. A milestone earns its place
only if it moves a user closer to owning the AI world they build.

**Current release:** `0.6.1` · **Status legend:** ✅ shipped · 🔶 partial · 🔭 planned · ❓ undecided

---

## Shipped milestones

The PRD framed M0–M4. M0–M3 are done; M4 is partial. Three milestones (palette,
suites v2, resource sources) were added after the PRD and are folded in below.

### M0 — Foundation ✅ `0.1.0`
Tauri 2.x + React/Vite/TS skeleton, `agentic-core` crate, settings at
`~/.agentic-hub/config.json`, empty-state shell.

### M1 — Core loop (scan / inspect / stage / apply) ✅ `0.1.0`–`0.1.1`
- Scanner, four tool adapters (Codex, Claude, Cursor, OpenClaw), planner, applier.
- Capability matrix (flat + tree), staged plan-then-apply, safe FS semantics.
- Demo scaffold (`0.1.1`), open-files actions (`0.1.1`), foreign-file conflict resolution (`0.1.1`).
- Hooks projection (`json_section`) and multi-source roots landed alongside.

**Exit met:** a user can replace the bash workflow for a real tool on a fresh macOS install.

### M2 — Suite presets ✅ `0.1.0`, hardened `0.4.0`
- Baseline named suites + Suite Manager + full-reset apply (`0.1.0`).
- Hardened in `0.4.0`: suite↔tool **bindings** with auto re-sync, **base suite**
  global merge, **source-qualified** refs for cross-device portability, palette suite apply.

**Exit met:** switch between named capability sets in one click; suites survive device sync.

### M3 — Workspace inventory (read-only) ✅ `0.5.0`
- **Direction change (2026-06-04):** the PRD's M3 hard-copied a suite *into* a
  project. That write path was removed (see [decisions](decisions.md) D7).
  Workspace scope is now a **read-only audit** of what each tool already has.
- Workspace target store (LRU), per-tool scan, live `workspace-changed` refresh,
  palette workspace search + locate.

**Exit met:** audit a project's installed resources without the hub ever writing into it.

### M4 — Launch hardening 🔶 in progress
- ✅ macOS DMG signed + notarized (`0.1.2`).
- 🔭 Linux `.deb` / `.AppImage` artifacts.
- 🔭 Integration tests against tmp dirs; Tauri boot/apply smoke test.
- ❓ Windows symlink path (constrained — see decisions D-open-1).

**Exit:** reproducible signed builds on every supported OS + a green integration suite.

### M+ — Command palette ✅ `0.3.0` (post-PRD)
Alfred-style floating window, global shortcut, command-provider registry, native
macOS menus. Extended by palette suite apply (`0.4.0`) and workspace locate (`0.5.0`).

### M+ — Resource sources ✅ `0.6.0`–`0.6.1` (post-PRD)
- skills.sh as the first pluggable public source behind a `SkillProvider` seam:
  search, local star/favorites, opt-in workspace install via controlled subprocess (`0.6.0`).
- Dedicated live-streaming **install window** + unified Global/Workspace scope rail (`0.6.1`).

---

## Forward milestones (proposed)

Uncommitted direction, ordered by mission leverage. Items are pulled from
[backlog.md](backlog.md); promote one here when it becomes the focused milestone.

### M5 — Resource channel ecosystem 🔭
skills.sh is "the first of several planned public resource channels." Make the
provider seam pay off: add a second source, a unified search surface across
sources, and richer install affordances (version pinning, update detection).

**Exit:** a user discovers and installs resources from ≥2 channels through one search.

### M6 — Cross-platform & distribution 🔭
Linux artifacts in CI, a decided Windows story (ship constrained vs. skip), and
auto-update via `tauri-plugin-updater` with hosted signed manifests.

**Exit:** non-macOS users install and auto-update without manual steps.

### M7 — Reliability & observability 🔭
Close M4's test debt, add opt-in rotating logs and perf instrumentation against
the NFR budgets (scan < 500ms p95, cold-start < 1.5s), and a telemetry decision.

**Exit:** the NFR budgets in PRODUCT.md are measured, not assumed.

---

## Version → milestone map

| Version | Date | Milestone | Headline |
|---------|------|-----------|----------|
| `0.1.0` | 2026-05-31 | M0/M1/M2 | First build: matrix, plan/apply, suites, watcher, multi-source |
| `0.1.1` | 2026-06-01 | M1 | Scaffold, open-files, foreign-file conflict resolution |
| `0.1.2` | 2026-06-02 | M4 | Signed + notarized macOS DMG |
| `0.2.0`–`0.2.1` | 2026-06-03 | M1 | Filter persistence, tree-default, actions-menu fixes |
| `0.3.0` | 2026-06-03 | M+ palette | Command palette + native menus |
| `0.4.0` | 2026-06-03 | M2 | Suite bindings, base suite, source-aware suites, palette apply |
| `0.5.0` | 2026-06-04 | M3 | Read-only workspace inventory + palette locate |
| `0.6.0` | 2026-06-04 | M+ sources | skills.sh source + opt-in install |
| `0.6.1` | 2026-06-04 | M+ sources | Install window + unified scope rail |

See [CHANGELOG.md](../../CHANGELOG.md) for the full per-version detail.
