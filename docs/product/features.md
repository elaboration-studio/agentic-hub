# Feature registry

> **Role of this doc.** The index of *what the product does*, at a glance: every
> shipped and planned capability, its status, the version it landed in, and a
> pointer to its spec. [PRODUCT.md](../../PRODUCT.md) holds the detailed feature
> *specs*; this registry is the management view over them.

**Status:** ✅ shipped · 🔭 planned · ❓ undecided
**Each feature serves the mission** — managing agentic resources (skills, agents,
rules, hooks) across tools from one place.

---

## Core projection engine

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| Capability matrix (flat + tree, search, filters) | ✅ | `0.1.0` | [mvp](../features/mvp-unified-agentic-capability-manager.md) |
| Plan-then-apply engine (safe FS, partial-tolerant) | ✅ | `0.1.0` | [projection arch](../../ARCHITECTURE.projection.md) |
| Tool adapter registry (Codex, Claude, Cursor, OpenClaw) | ✅ | `0.1.0` | [tool-adapter-matrix](../tech/reference/tool-adapter-matrix.md) |
| Rule instruction sync (3 modes, managed block) | ✅ | `0.1.0` | [rule-projection-sync](../tech/modules/rule-projection-sync.md) |
| Hooks projection (`json_section`) | ✅ | `0.1.x` | [hooks-projection](../features/hooks-projection.md) |
| Multi-source roots (priority, first-wins, `__archived__`) | ✅ | `0.1.0` | [multi-source-roots](../tech/modules/multi-source-roots.md) |
| Source watcher (auto-reconcile, Watch toggle) | ✅ | `0.1.0` | [source-watcher](../features/source-watcher.md) |
| Foreign-file conflict resolution (explicit takeover) | ✅ | `0.1.1` | D3 in [decisions](decisions.md) |
| Demo scaffold (first-run bootstrap) | ✅ | `0.1.1` | [agentic-demo-scaffold](../features/agentic-demo-scaffold.md) |
| Open files (preferred editor / reveal / tool target) | ✅ | `0.1.1` | [open-files](../features/open-files.md) |

## Suites

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| Suite presets (Suite Manager, full-reset apply) | ✅ | `0.1.0` | [suite-presets](../features/suite-presets.md) |
| Suite↔tool bindings (auto re-sync on edit) | ✅ | `0.4.0` | [suite-bindings](../tech/modules/suite-bindings.md) |
| Base suite (global merge) + suite-lock | ✅ | `0.4.0` | [suite-bindings](../tech/modules/suite-bindings.md) |
| Source-qualified suite refs (cross-device portability) | ✅ | `0.4.0` | D10 in [decisions](decisions.md) |

## Workspace

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| Read-only workspace inventory (per-tool scan) | ✅ | `0.5.0` | [workspace-inventory](../features/workspace-inventory.md) |
| Live workspace refresh (`workspace-changed`) | ✅ | `0.5.0` | [watcher](../tech/modules/watcher.md) |
| Unified Global/Workspace scope rail | ✅ | `0.6.1` | D12 in [decisions](decisions.md) |

## Command palette

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| Alfred-style palette + global shortcut + native menus | ✅ | `0.3.0` | [command-palette](../features/command-palette.md) |
| Command-provider registry (resource + nav) | ✅ | `0.3.0` | [command-palette](../features/command-palette.md) |
| Palette suite apply (two-level) | ✅ | `0.4.0` | [command-palette](../features/command-palette.md) |
| Palette workspace search + locate | ✅ | `0.5.0` | [command-palette](../features/command-palette.md) |

## Resource sources

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| skills.sh source (search, star, opt-in install) | ✅ | `0.6.0` | [skills-sh-integration](../features/skills-sh-integration.md) |
| Pluggable `SkillProvider` seam | ✅ | `0.6.0` | [skill-sources](../tech/modules/skill-sources.md) |
| Live-streaming install window (+ Cancel) | ✅ | `0.6.1` | [skills-sh-integration](../features/skills-sh-integration.md) |
| Second resource channel | 🔭 | M5 | backlog X1 |
| Unified cross-source search | 🔭 | M5 | backlog X2 |
| Installed-resource update detection | 🔭 | M5 | backlog X3 |

## Platform & distribution

| Feature | Status | Since | Spec |
|---------|--------|-------|------|
| Config page (per-tool paths, editor, suite/skills config) | ✅ | `0.1.0` | [PRODUCT.md](../../PRODUCT.md) |
| App-wide color scheme (light, dark, follow system) | ✅ | `0.13.0` | [color-scheme](../features/color-scheme.md) |
| Local usage tracing collector health checks and recovery | ✅ | `0.13.1` | [local usage tracing](../features/local-skill-usage-tracing.md) |
| macOS signed + notarized DMG pipeline | ✅ | `0.1.2` | [DEPLOYMENT.md](../../DEPLOYMENT.md) |
| Linux `.deb` / `.AppImage` artifacts | 🔭 | M6 | backlog X4 |
| Auto-update (`tauri-plugin-updater`) | 🔭 | M6 | backlog X5 |
| Windows support | ❓ | — | D-open-1 |

---

## How to use this registry

- **Adding a feature?** Add a row with status, target version/milestone, and a
  spec link the moment work starts (🔭), flip to ✅ when it ships.
- **One source of truth per fact:** behavior detail lives in the linked spec;
  *why* lives in [decisions](decisions.md); *when* lives in [roadmap](roadmap.md);
  *what's next* lives in [backlog](backlog.md). This file only indexes them.
