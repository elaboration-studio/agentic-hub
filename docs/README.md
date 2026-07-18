# Agentic Hub Docs

Detailed documentation for Agentic Hub. Start with the root-level specs, then drill into the buckets below.

## Root specs

- [PRODUCT.md](../PRODUCT.md) — product requirements, user stories, scope, milestones
- [ARCHITECTURE.md](../ARCHITECTURE.md) — system architecture, stack, components, key flows
- [ARCHITECTURE.permissions.md](../ARCHITECTURE.permissions.md) — Tauri capability model, FS trust boundary, Windows constraint
- [ARCHITECTURE.projection.md](../ARCHITECTURE.projection.md) — projection engine (scan → plan → apply → sync)
- [ARCHITECTURE.workspace.md](../ARCHITECTURE.workspace.md) — workspace patch lifecycle, manifest cycle, safety guards

## Buckets

### `product/` — living product management

The management layer over the static [PRODUCT.md](../PRODUCT.md) PRD: where the
product is, where it's going, and why.

- [roadmap.md](product/roadmap.md) — milestone tracker (M0–M4 retro + forward milestones, version map)
- [backlog.md](product/backlog.md) — prioritized Now / Next / Later backlog + tech debt
- [decisions.md](product/decisions.md) — decision log (why the big calls were made; open questions)
- [features.md](product/features.md) — feature registry indexing every shipped/planned capability

### `features/` — current feature specs

User-facing feature designs aligned with each milestone.

- [mvp-unified-agentic-capability-manager.md](features/mvp-unified-agentic-capability-manager.md) — the core MVP feature: scan / inspect / stage / apply
- [hooks-projection.md](features/hooks-projection.md) — the `hook` capability kind projected into each tool's hooks config via `json_section`
- [commands.md](features/commands.md) — the `command` capability kind (slash-command prompts) projected into each tool's commands dir; palette copy / open
- [suite-presets.md](features/suite-presets.md) — named capability presets with one-click full-reset apply
- [workspace-inventory.md](features/workspace-inventory.md) — read-only per-workspace inventory of installed agentic resources
- [global-installed-resources.md](features/global-installed-resources.md) — read-only Global Manager audit of resources installed directly into tool folders
- [skills-sh-integration.md](features/skills-sh-integration.md) — opt-in skills.sh source: search, star, and install skills into a workspace
- [source-watcher.md](features/source-watcher.md) — auto-reconcile projections on source-root file changes; Watch toggle
- [agentic-demo-scaffold.md](features/agentic-demo-scaffold.md) — first-run bootstrap of a starter shared root
- [open-files.md](features/open-files.md) — per-row actions to open a capability's original file (preferred editor) or the file each tool actually references
- [kiro-tool-adapter.md](features/kiro-tool-adapter.md) — opt-in Kiro projection and workspace inventory
- [copilot-tool-adapter.md](features/copilot-tool-adapter.md) — opt-in GitHub Copilot projection and workspace inventory
- [antigravity-tool-adapter.md](features/antigravity-tool-adapter.md) — opt-in Google Antigravity projection and workspace inventory
- [local-skill-usage-tracing.md](features/local-skill-usage-tracing.md) — local skill usage counts (enabled by default, opt-out in Config) surfaced in the Manager matrix and Statistics tabs
- [color-scheme.md](features/color-scheme.md) — app-wide light, dark, and follow-system appearance

### `tech/modules/` — subsystem deep dives

Detailed technical design for each subsystem of the projection engine and adjacent services.

- [rule-projection-sync.md](tech/modules/rule-projection-sync.md) — three rule projection modes; managed-block contract
- [hook-projection-sync.md](tech/modules/hook-projection-sync.md) — the `json_section` mode: hook schema, event mapping, marker-preserving JSON CRUD
- [multi-source-roots.md](tech/modules/multi-source-roots.md) — ordered source forest, first-source-wins dedupe, priority collision resolution
- [claude-flat-skill-layout.md](tech/modules/claude-flat-skill-layout.md) — Claude's flat layout constraint and basename-collision rules
- [openclaw-tool-adapter.md](tech/modules/openclaw-tool-adapter.md) — OpenClaw filesystem conventions and SOUL.md managed block
- [kiro-tool-adapter.md](tech/modules/kiro-tool-adapter.md) — Kiro paths and per-hook JSON projection
- [copilot-tool-adapter.md](tech/modules/copilot-tool-adapter.md) — Copilot paths, filename transforms, and per-hook JSON projection
- [antigravity-tool-adapter.md](tech/modules/antigravity-tool-adapter.md) — Antigravity paths and aggregate hook/rule projection
- [suite-presets.md](tech/modules/suite-presets.md) — suite store, full-reset apply pipeline
- [suite-bindings.md](tech/modules/suite-bindings.md) — suite↔tool binding store; auto re-sync on capability edits
- [workspace-inventory.md](tech/modules/workspace-inventory.md) — workspace target store, per-tool scan, read-only inventory contract
- [skill-sources.md](tech/modules/skill-sources.md) — pluggable skill-source seam, skills.sh provider, local favorites store, install path
- [cli-tools.md](tech/modules/cli-tools.md) — Tools preflight: bundled tool catalog (+ user override), login-PATH probe with timeout, install/auth status
- [watcher.md](tech/modules/watcher.md) — source watcher + reconcile engine, auto-enable heuristic, debounce, loop avoidance
- [tauri-ipc-contract.md](tech/modules/tauri-ipc-contract.md) — complete IPC command surface and event schemas
- [agentic-demo-scaffold.md](tech/modules/agentic-demo-scaffold.md) — bundled tree embedding, scaffold modes, atomic writes
- [local-usage-tracing.md](tech/modules/local-usage-tracing.md) — local SQLite usage store, loopback collector, and skill-count query contract

### `tech/reference/` — stable reference

Quick-lookup reference tables.

- [shared-root-contract.md](tech/reference/shared-root-contract.md) — `~/.agentic` filesystem contract, validation rules
- [tool-adapter-matrix.md](tech/reference/tool-adapter-matrix.md) — per-tool projection matrix (target paths × layouts × projection modes)

### `tech/development/` — engineering workflows

Day-to-day development reference.

- [getting-started.md](tech/development/getting-started.md) — local setup, scripts, common workflows
- [run-test-debug.md](tech/development/run-test-debug.md) — verified cheat sheet: how to start, test, and debug the app
- [testing-strategy.md](tech/development/testing-strategy.md) — test pyramid, fixtures, coverage targets, CI matrix

### `plans/` — migration & change plans

Time-boxed plans that coordinate multi-doc or multi-module change.

- [vscode-extension-feature-migration_2026-05-31.plan.md](plans/vscode-extension-feature-migration_2026-05-31.plan.md) — port hooks, multi-source roots, and `__archived__` scan exclusion from the VS Code extension (`0.3.0`–`0.5.0`) into the hub

## How to read this

| If you are… | Start with |
|-------------|------------|
| New to the product | `PRODUCT.md` → `ARCHITECTURE.md` |
| Tracking status, priorities, or "why" | `product/roadmap.md` · `product/backlog.md` · `product/decisions.md` · `product/features.md` |
| About to implement a feature | The matching `docs/features/*.md` → linked tech modules |
| Touching the projection engine | `ARCHITECTURE.projection.md` → relevant `tech/modules/*.md` |
| Adding a tool adapter | `tech/reference/tool-adapter-matrix.md` → `claude-flat-skill-layout.md` for the layout pattern |
| Adding an IPC command | `tech/modules/tauri-ipc-contract.md` → `getting-started.md` (workflow patterns section) |
| Writing or reviewing tests | `tech/development/testing-strategy.md` |
