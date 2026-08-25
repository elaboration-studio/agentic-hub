# Product backlog

> **Role of this doc.** The single prioritized list of *what's next*. Items flow
> **Now → Next → Later** (or **Not planned**). When an item becomes the focused
> milestone it graduates to [roadmap.md](roadmap.md); when it ships it lands in
> [features.md](features.md) and the [CHANGELOG](../../CHANGELOG.md).

**Priority key:** `P0` must-have · `P1` important · `P2` nice-to-have.
**Every item names the mission link** — if it doesn't help someone better manage
their agentic resources, it doesn't belong here.

---

## Now — committed / in flight

| ID | P | Item | Why (mission link) | Notes |
|----|---|------|--------------------|-------|
| N1 | P0 | Integration tests vs tmp dirs | Trust the plan/apply engine never clobbers real files | Closes M4 test debt; see [testing-strategy](../tech/development/testing-strategy.md) |
| N2 | P0 | Tauri boot/apply smoke test | Catch packaging/IPC regressions before release | Boot window → apply on tmp root → assert disk state |
| N3 | P1 | Version reconciliation | Keep release metadata honest | `0.7.0` plan folded into `0.6.1`; verify `package.json`/`tauri.conf.json`/Cargo all read `0.6.1` |

## Next — planned, not started

| ID | P | Item | Why (mission link) | Notes |
|----|---|------|--------------------|-------|
| X1 | P1 | Second resource channel | Prove the `SkillProvider` seam; more building blocks reachable | Drives M5; skills.sh was "first of several" |
| X2 | P1 | Unified cross-source search | One search over every channel = less friction collecting resources | Depends on X1 |
| X3 | P1 | Installed-resource update detection | Know when a workspace resource drifts from upstream | Read-only signal first, install action second |
| X4 | P1 | Linux `.deb` / `.AppImage` in CI | Non-macOS users can run the hub at all | Drives M6 |
| X5 | P1 | Auto-update (`tauri-plugin-updater`) | Users get fixes without re-downloading | Needs hosted signed manifests (decision D-open-2) |
| X6 | P2 | API key in OS keychain | Don't leave secrets plaintext in `config.json` | Tech-debt T1; current state is intentional v1 tradeoff |

## Later — parked, revisit when triggered

| ID | P | Item | Why (mission link) | Trigger |
|----|---|------|--------------------|---------|
| L1 | P2 | Windows symlink/managed-copy path | Windows users can manage resources | Decide ship-vs-skip (D-open-1) before any Windows release |
| L2 | P2 | OpenClaw workspace support | Parity for the 4th tool in workspace scope | When OpenClaw's project scan path stabilizes |
| L3 | Done | Opt-in usage telemetry | Optimize the personal-tool experience with real data | Shipped: opt-in Aptabase, off by default, Rust-only lifecycle events (D15) |
| L4 | P2 | Suite import/export & composition | Share/compose known-good capability sets | Demand from dogfooding |
| L5 | P2 | Cross-tool consistency inspector | Audit which tools have an item at a glance | PRD P2 story; pull forward if drift becomes painful |

## Not planned (explicit non-goals)

Kept here so they aren't silently re-proposed. From PRODUCT.md *Out of Scope*:

- Teams / org-level capability libraries.
- Multi-machine sync or cloud profiles (source-qualified suites already give
  *portable* files synced by the user's own tool — see decisions D10).
- Background drift detection / watch-mode for workspace files.
- Per-tool overrides inside a suite definition.

---

## Tech debt

Tracked here per the repo convention (the refactor workflow updates this section).

| ID | Item | Risk | Disposition |
|----|------|------|-------------|
| T1 | skills.sh API key stored plaintext in `config.json` | Low (local-only, single-user) | Accepted for v1; keychain is X6 |
| T2 | 600-line-per-file cap (workspace convention) | Medium | Watch `commands.rs`, `Matrix.tsx`, `api.rs` as features land |
| T3 | NFR budgets unmeasured (scan p95, cold-start) | Medium | Instrument in M7 (N-series first) |
| T4 | Stale PRD milestones vs shipped reality | Low | Mitigated by this product/ bucket; refresh PRD at next major |

---

## How items move

```
idea ──▶ Backlog (Later) ──▶ Next ──▶ Now ──▶ Roadmap milestone ──▶ shipped
                                              │
                                              └──▶ features.md + CHANGELOG
```

Add an item with: an `ID`, a priority, a one-line **why** tied to the mission,
and (for Now/Next) a pointer to the spec or decision that backs it.
