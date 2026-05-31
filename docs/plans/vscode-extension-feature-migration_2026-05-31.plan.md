# Plan: VS Code Extension → Agentic Hub Feature & Docs Migration

Status: Draft
Mode: Detailed (plan only — no code in this pass)
Owner: Arno
Last Updated: 2026-05-31
Depends On: [PRODUCT.md](../../PRODUCT.md), [ARCHITECTURE.md](../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../features/mvp-unified-agentic-capability-manager.md), [docs/tech/reference/tool-adapter-matrix.md](../tech/reference/tool-adapter-matrix.md), [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md), [docs/tech/modules/rule-projection-sync.md](../tech/modules/rule-projection-sync.md)

## Purpose

The Agentic Hub docs were forked from the VS Code extension at its **~0.2.0** state (every hub doc reads `Last Updated: 2026-05-20`). The extension has since shipped three more feature waves (`0.3.0`–`0.5.0`). This plan reconciles that drift: it decides which extension changes belong in the Tauri-native hub, translates them from the VS Code service model into the `agentic-core` (Rust) + Tauri IPC + React model, and lists the exact hub docs to create or update.

This is a **plan only**. No Rust, TypeScript, or capability JSON is written in this pass — the deliverables here are design, docs, and specs.

## Baseline: what the hub docs already capture

The hub docs already mirror the extension through `0.2.0`:

- Scan / inspect / plan / apply pipeline (`scanner`, `adapter_registry`, `planner`, `applier`)
- Three projection modes: `link_sync`, `file_sync` (managed copy), `markdown_section_sync`
- Three capability kinds: `skill`, `agent`, `rule`
- Four tool adapters: Codex, Claude Code, Cursor, OpenClaw
- Claude flat layout + basename collision pass
- Cursor managed-copy lifecycle with `.e-studio-meta.json` sidecars
- Suite presets, workspace suite sync, demo scaffold
- Single `sharedRoot` setting

## Drift: what shipped after the fork

| Version | Date | Change | Verdict | Why |
|---------|------|--------|---------|-----|
| `0.3.0` | 05-25 | **Hooks as a first-class capability** (`hook` kind, `json_section` projection, per-tool hook config sync, `_agenticHub` marker, `${HOOK_DIR}`, `loopLimit`, Not-Targeted UX, workspace hook sync, demo hook) | **Migrate** | Core projection-engine capability; same FS-trust model applies |
| `0.4.0` | 05-26 | **Multi-source capability roots** (`sources` list, `SourceConfig`, `scanAll`, cross-source collision by priority, `sourceId`/`sourceLabel`, Add/Remove Source UI) | **Migrate** | Changes the scanner + planner + settings contract; tool-agnostic |
| `0.4.1` | 05-27 | Settings button + `sources` schema fixes | **Skip** | VS Code Settings-UI specific; the hub has its own settings surface |
| `0.5.0` | 05-29 | `__archived__` folders excluded from scans | **Migrate** | Pure scanner walk rule; tool-agnostic |
| `0.5.0` | 05-29 | Copy Open File as Editor Link | **Skip** | Editor command (`vscode://`/`cursor://`); the hub is not a text editor |
| `0.5.0` | 05-29 | `.sop` resource link type | **Skip** | Markdown link/completion taxonomy in the editor, not a projection kind |
| `0.5.0` | 05-29 | Clickable `~/` picture & file links | **Skip** | Markdown editing affordance in VS Code |
| `0.2.0` and earlier | — | `/ah-*` & `/quote`/`/note` slash commands, markdown link navigation, E-Studio remote API, copy-file-context | **Skip** | All editor/remote-API surface; explicitly excluded from the hub product |

Net: **three** changes migrate (hooks, multi-source, `__archived__` exclusion). Everything in `0.5.0` except the scan rule is editor-only and out of scope by design — the hub has no text-editor surface.

---

## Migration 1 — Hooks projection

The largest migration. A fourth capability kind with a fourth projection mode that writes into each tool's native hook JSON without clobbering hand-authored entries.

Source docs: `docs/features/hooks-projection.md`, `docs/tech/modules/hook-projection-sync.md` (extension repo).

### Service → core translation

| VS Code extension | Agentic Hub (`agentic-core`) |
|-------------------|------------------------------|
| `HookProjectionSyncService` | new `hook_sync` module (sibling of `rule_sync`) |
| `HookEventMapper` | `hook_sync::event_map` submodule |
| hook manifest loader in scan service | `scanner` gains `hook` kind walk + `HookManifest` parse/validate |
| `_agenticHub` inline marker | preserved verbatim (parity) |
| `sync_json_section` / `clear_json_section` ops | new `PlannedOperation` kinds |

### Capability kind & projection mode

- `CapabilityKind` gains `Hook`. Scan walk: `<source>/hooks/**/hook.json` → `CapabilityKind::Hook`, `source_path` = the hook's containing folder (so `${HOOK_DIR}` resolves to it).
- New projection mode `json_section` joins `link_sync` / `file_sync` / `markdown_section_sync`. It is the only mode for `hook`, and only `hook` uses it.
- `ProjectionKind` (the per-op tag) gains `json_section`; new ops `sync_json_section` and `clear_json_section`.

### `hook_sync` module contract (spec)

```rust
// HookManifest mirrors the extension schema 1:1
pub struct HookManifest {
    pub id: String,                  // kebab-case, unique within source
    pub name: Option<String>,
    pub description: Option<String>,
    pub events: Vec<HookEventSpec>,  // 1..N
    pub command: String,             // may contain ${HOOK_DIR}
    pub timeout: Option<u32>,        // seconds
    pub loop_limit: Option<u32>,     // Cursor-only safety knob; positive int
    pub targets: Option<Vec<ToolId>>,// default ["cursor","claude","codex"]
}

pub enum HookSyncOutcome { Wrote, Removed, NoOp }
pub enum HookSyncError { ForeignFile(PathBuf), Broken(PathBuf), PermissionDenied(PathBuf), Io(std::io::Error) }

// read → partition (mine vs theirs by _agenticHub) → rebuild managed set → atomic write
pub fn sync_json_hooks(input: HookSyncInput) -> Result<HookSyncOutcome, HookSyncError>;
```

Behavior to preserve verbatim from the extension module:

- **Co-existence by marker.** Every managed entry carries `_agenticHub { hookId, sourceHash, version }`. On each sync, read the whole file, partition into managed vs foreign, rebuild only managed; write foreign back verbatim including surrounding keys (`permissions`, `env`, etc. in Claude's `settings.json`).
- **`sourceHash`** = `sha256` of the canonical manifest JSON (minus `$schema`, stable key order).
- **Two shapes.** Cursor `hooks.json` is flat (`{ version, hooks: { <camelCase>: [...] } }`); Claude `settings.json` and Codex `hooks.json` are two-level with the marker on a dedicated matcher group (never merge into a user matcher group).
- **Event mapping** lives in one place: canonical PascalCase ↔ Cursor camelCase, plus per-tool supported-event sets. Unsupported `(event, tool)` combos are **not errors** — they emit a per-tool note in the apply result.
- **`${HOOK_DIR}`** expands at projection time to the hook source folder's absolute path.
- **Atomic write** (temp + rename). Empty managed set does not delete a file that has foreign keys; delete only when the result is empty AND the file is hooks-only with no foreign top-level keys.
- **State inspection** from file contents: missing file → `disabled`; non-file → `foreign_file`; parse error → `broken`; managed entry with matching `sourceHash` → `enabled`; mismatched → `stale`; none → `disabled`.

### Adapter registry additions

Per-tool hook settings (mirror extension keys, hub-native naming):

- `hooks_enabled: bool` — default `true` for Codex/Claude/Cursor, `false` for OpenClaw.
- `hooks_file: Option<PathBuf>` — `~/.cursor/hooks.json`, `~/.claude/settings.json`, `~/.codex/hooks.json`; OpenClaw has none.
- A hook's effective targets = `manifest.targets` ∩ `{tools where hooks_enabled}`.

**Not-Targeted contract** (carry over the 0.3.0 fix): when a focused tool is not in a hook's `targets` (or the tool's `hooks_enabled` is off), the item is **locked** in that tool's view with a `Not Targeted` chip. The plan payload is sanitized by a `filter_desired_enabled_for_tool` step **before** planning, so a stale toggle can never produce a silent no-op; each dropped item yields one descriptive note. Parent/bulk-toggle counts use an **applicable count** (children whose targets include the focused tool), not the raw child count.

### IPC additions (tauri-ipc-contract)

- `CapabilityKind` union gains `'hook'`; `ToolCapabilityState.state` already covers the needed states.
- `PlannedOperation.kind` gains `'sync_json_section' | 'clear_json_section'`.
- `cmd_plan` / `cmd_apply` flow unchanged in shape; a tool apply batches all hook ops into one read-merge-write per target file.
- New error codes: `hook_manifest_invalid`, `hook_target_broken_json` (maps to `broken`).
- `Settings` / `ToolSettings` gain `hooksEnabled` and `hooksFile`.

### Workspace integration

Workspace hook projection writes `<ws>/.cursor/hooks.json`, `<ws>/.claude/settings.json`, `<ws>/.codex/hooks.json`. The workspace manifest records a `<target-rel-path>::managed-hooks` sentinel so the next apply clears prior managed hook entries before writing the new set — same pattern as the existing `::managed-section` rule sentinel.

### Demo scaffold

The bundled demo tree gains an `auto-format-after-edit` hook (`hook.json` + `script.sh`) so the end-to-end hook flow is dogfoodable on first run.

### Parity hard rules (do not rename)

- Inline marker key stays `_agenticHub`.
- `${HOOK_DIR}` token, `loopLimit` field, `agentic-hub.hook.v1` `$schema` value — all verbatim.

### Docs to produce

| Doc | Action |
|-----|--------|
| `docs/features/hooks-projection.md` | **Create** — port the extension feature doc; reframe "VS Code Capability Manager" → "Agentic Hub window", keep schema/tool-coverage/co-existence/state tables |
| `docs/tech/modules/hook-projection-sync.md` | **Create** — port the module doc; replace TS snippets with the Rust `hook_sync` spec above; keep event-map table, two-shape JSON, inspection + atomic-write algorithm |
| `docs/tech/reference/tool-adapter-matrix.md` | **Update** — add a Hooks row block (target files, shape, `hooksEnabled` defaults, OpenClaw = none); extend the projection-mode + ops tables with `json_section` |
| `docs/tech/modules/tauri-ipc-contract.md` | **Update** — `hook` kind, new op kinds, new error codes, hook settings fields |
| `docs/features/mvp-unified-agentic-capability-manager.md` | **Update** — add `hook` to capability kinds, kind filter, Not-Targeted state; note it is a post-MVP kind |
| `ARCHITECTURE.projection.md` | **Update** — scan walk gains the hooks line; "projection modes" count 3 → 4; new section "JSON-section managed-entry contract" |
| `docs/tech/modules/agentic-demo-scaffold.md` | **Update** — list the demo hook |
| `docs/tech/modules/workspace-patch.md` | **Update** — document the `::managed-hooks` sentinel |

---

## Migration 2 — Multi-source capability roots

Replace the single `sharedRoot` with an ordered list of sources, each its own inventory root, with priority-based collision resolution. Tool-agnostic; touches settings, scanner, planner, IPC, UI.

Source doc: `docs/tech/modules/multi-source-roots.md` (extension repo).

### Contract

```rust
pub struct SourceConfig {
    pub id: String,    // stable slug derived from label (deterministic de-dup)
    pub label: String, // tree root + message label
    pub path: PathBuf, // absolute, normalized
}
```

- Persisted as `sources: [{ label, path }]` in `~/.agentic-hub/config.json`. A pure helper (`resolve_source_configs`) turns the persisted pairs into stable-slug `SourceConfig`s with deterministic ID de-dup — analog of the extension's `agenticSourceConfig.resolveSourceConfigs`.
- **Legacy fallback:** when `sources` is empty, synthesize a single `Default` source from the existing `sharedRoot`. `sharedRoot` is marked deprecated and kept for one release. No migration required for existing installs.

### Scanner

- `scan_all(sources: &[SourceConfig]) -> ScanResult` becomes the primary entry; `scan(shared_root)` stays as a thin wrapper.
- Walk every source in priority order; key items by `${kind}:${relative_path}`; **first source wins**. Shadowed (later-source) duplicates are dropped from inventory and surfaced as a `ScanError` so the user can rename or reorder.
- `CapabilityItem` gains required `source_id` / `source_label` (surfaced in the inspector "Source" row).

### Planner

Generalize the existing flat-layout collision pass into a **projection-target collision pass** that covers all cross-source target clashes (not only Claude flat-layout basenames). Sort enabled items by source priority; the higher-priority source owns the target; the loser becomes a `skip_conflict` whose reason names the winning source. The existing Claude-flat case is subsumed by this pass (behavior preserved).

### Identity strategy (keep flat)

Capability IDs stay source-free (`skill:dev/tdd`, `rule:general/precise.mdc`). Consequences, both intended:

- Existing `~/.agentic-suites.json` suites keep working with **no migration**; an ID resolves to whichever source currently owns it.
- Adding/reordering sources never changes an ID — only which source backs it.

### IPC additions

- `cmd_scan` takes `sources: SourceConfig[]` (legacy single-root call kept as a wrapper).
- `ScanError` already carries `path` + `message`; shadowing reuses it.
- New commands: `cmd_add_source(label, path)`, `cmd_remove_source(id)` (folder dialog handled in the shell), persisting to settings and emitting `settings-changed`.
- `CapabilityItem` type gains `sourceId` / `sourceLabel`.
- New error code: `source_path_invalid`.

### UI

- Tree shows one root node per source (label from `SourceConfig`); each holds Skills / Agents / Rules / Hooks. Search collapses the forest to a single synthetic "All results" root (existing behavior over `node.itemIds` is unchanged).
- Header `Add Source…` (pick folder → label → validate uniqueness → append). Per-source inspector `Remove Source` (modal-confirmed; never touches disk). Removing the only source falls back to legacy `sharedRoot`.
- Demo scaffold writes into `sources[0].path` (falling back to `sharedRoot`).

### Docs to produce

| Doc | Action |
|-----|--------|
| `docs/tech/modules/multi-source-roots.md` | **Create** — port the extension module; replace VS Code settings/`showOpenDialog` references with `~/.agentic-hub/config.json` + Tauri dialog + IPC; keep contract, order-is-priority, identity, tree-shape, backward-compat sections |
| `docs/tech/reference/shared-root-contract.md` | **Update** — from "the shared root" to "an ordered list of sources, each a shared root"; document the `kind:relative_path` dedup key and shadow error |
| `docs/features/mvp-unified-agentic-capability-manager.md` | **Update** — header shows sources (not a single root); "Multi-source roots" subsection; empty-state and tree changes |
| `docs/tech/modules/tauri-ipc-contract.md` | **Update** — `cmd_scan(sources)`, `cmd_add_source`, `cmd_remove_source`, `sourceId`/`sourceLabel`, `Settings.sources` |
| `ARCHITECTURE.projection.md` | **Update** — Stage 1 scan becomes multi-source; the collision pass generalizes from flat-layout to projection-target |
| `docs/tech/modules/suite-presets.md` / `docs/tech/modules/workspace-patch.md` | **Update** — note suite IDs resolve against the source forest (first-wins); stale-reconciliation messages read "any configured source" |

---

## Migration 3 — `__archived__` scan exclusion

Per the Frame.meta convention, `__archived__` holds old versions of files. The scanner walk skips any directory named `__archived__` (at any depth) so archived capabilities never surface in inventory, suites, or plans.

### Docs to produce

| Doc | Action |
|-----|--------|
| `ARCHITECTURE.projection.md` | **Update** — Stage 1 scan: add the `__archived__` skip rule to the walk strategy |
| `docs/tech/reference/shared-root-contract.md` | **Update** — document `__archived__` as a reserved, ignored folder name |

---

## Out of scope (editor-only, by design)

These extension features depend on a live text editor / markdown buffer that the Tauri hub does not have. They are intentionally **not** migrated; record the rationale here so the gap is a decision, not an oversight.

| Extension feature | Reason not applicable |
|-------------------|-----------------------|
| Copy Open File as Editor Link (`copyOpenFileUri`) | Reads `activeTextEditor` + `vscode.env.uriScheme`; no editor in the hub |
| `.sop` resource link type | A markdown link/completion taxonomy resolved in the editor, not a projection capability kind |
| Clickable `~/` picture & file links | Markdown rendering affordance in VS Code |
| `/ah-*`, `/quote`, `/note` slash commands | Markdown-buffer commands |
| Markdown link navigation, E-Studio remote API, copy-file-context | Editor + remote-sync surface already excluded from the hub product (removed at rebrand) |

If a future hub feature wants file-deep-linking (e.g. "open this capability's source in the user's editor"), it would be a **new** hub feature, not a port — track separately.

---

## Consolidated docs migration map

| Source (extension `docs/`) | Target (`agentic-hub/docs/`) | Action |
|----------------------------|------------------------------|--------|
| `features/hooks-projection.md` | `features/hooks-projection.md` | Create (port) |
| `tech/modules/hook-projection-sync.md` | `tech/modules/hook-projection-sync.md` | Create (port, Rust spec) |
| `tech/modules/multi-source-roots.md` | `tech/modules/multi-source-roots.md` | Create (port, Tauri/IPC) |
| `tech/modules/copy-open-file-uri.md` | — | Skip (editor-only) |
| `tech/modules/markdown-resource-links.md` | — | Skip (editor-only) |
| MVP "Multi-source roots" section | `features/mvp-unified-agentic-capability-manager.md` | Update |
| — | `tech/reference/tool-adapter-matrix.md` | Update (hooks rows + `json_section`) |
| — | `tech/modules/tauri-ipc-contract.md` | Update (hooks + sources) |
| — | `ARCHITECTURE.projection.md` | Update (scan, modes, collision, `__archived__`) |
| — | `tech/reference/shared-root-contract.md` | Update (sources, `__archived__`) |
| — | `tech/modules/workspace-patch.md` | Update (`::managed-hooks` sentinel) |
| — | `tech/modules/agentic-demo-scaffold.md` | Update (demo hook) |
| — | `docs/README.md` | Update (link new docs + this plan) |

Every touched hub doc should bump its `Last Updated` to the migration date and keep the existing header block shape (`Status / Mode / Owner / Last Updated / Depends On / Related Docs`).

---

## Sequencing

| Phase | Scope | Rationale |
|-------|-------|-----------|
| **A. This plan** | Land this plan doc | Single source of truth before any doc/code churn |
| **B. `__archived__`** | Scanner walk rule + 2 doc edits | Trivial, isolated, unlocks clean inventory; no contract change |
| **C. Hooks projection** | New kind + mode + `hook_sync` + adapter/IPC/UI + 8 docs | Self-contained new kind; does not change existing scan/plan contracts for skill/agent/rule |
| **D. Multi-source roots** | Settings + scanner + planner + IPC + UI + 6 docs | Changes the scan/plan entry contract; do last so hooks land against the simpler single-root model first, then both generalize to the forest together |

Doc-first within each phase: write/refresh the feature + module + matrix + IPC docs, regenerate the IPC contract mentally against `ts-rs`, then (in a later, code-permitted pass) implement and add tests at the layer the testing-strategy doc prescribes.

## Parity & hard rules carried through

- Managed markers verbatim: `<!-- e-studio-agentic-rules:start -->` / `:end`, and the hook `_agenticHub` marker key.
- Suite storage path stays `~/.agentic-suites.json`; workspace manifest folder stays `<ws>/.agentic-hub/`.
- `tauri-plugin-shell` is never added — hooks are projected as **config entries**, never executed by the hub.
- Every new IPC command must appear in `src-tauri/capabilities/default.json` when implemented.
- Every path parameter (source paths, hook files) canonicalized via the central validator before any FS op.
- Capability IDs remain source-free so existing suites need no migration.

## Open questions

- **Hook execution trust.** The hub only *writes* hook config; each tool owns its own trust dialog (Codex `/hooks`, Cursor team enforcement). Confirm we surface a one-line reminder in the hooks feature doc rather than attempting to manage trust.
- **Per-source enable/disable toggle.** The extension scans every configured source. Defer a per-source on/off toggle and drag-reorder UI to a follow-up (noted in the extension doc's "Future Extensions").
- **Workspace-scoped sources.** Currently sources are global-only. Layering workspace sources on top is a future extension, not part of this migration.
- **`$schema` value for hub hooks.** Keep `agentic-hub.hook.v1` from the extension, or namespace differently? Default: keep for parity.
