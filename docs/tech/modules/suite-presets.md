# Module: Suite Presets

Status: Implemented
Mode: Detailed
Owner: Arno
Last Updated: 2026-07-31
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/features/suite-presets.md](../../features/suite-presets.md)
Related Docs: [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/modules/workspace-inventory.md](./workspace-inventory.md)

## Overview

The Suite Presets module adds named capability presets to Agentic Hub. A suite is a tool-agnostic list of capability IDs. Applying a suite to a chosen target tool fully resets that tool's enabled capabilities to match the suite definition, then runs the existing projection pipeline.

The module reuses the existing scan, plan, apply, and rule-sync code. Suite CRUD now lives directly in the Manager's `suite` scope and reuses the same capability table frame as Global and Workspace.

## Background and problem

The MVP manager enables per-item toggling. This works for small deltas but forces users to manually reconstruct entire configurations when switching scenarios. Suites solve this by letting users define named configurations once and apply them repeatedly.

## Goals

- Let users save named capability presets
- Let users apply a preset to any tool in one action
- Fully reset the tool's state on apply — no leftover capabilities from the previous configuration
- Reuse the existing apply pipeline without forking it

## Non-goals

- Per-tool overrides inside a suite
- Suite versioning, history, or undo
- Suite import/export
- Automatic suite application (always user-initiated)
- Suite composition (merging multiple suites)

## Scope and boundaries

### In scope

- Suite data model and types
- `~/.agentic-suites.json` storage contract (preserved path from VS Code extension)
- `suite_store` for CRUD and validation
- Manager rail Suite section and shared-table editor
- Compatibility navigation aliases for the former Suites route
- New IPC commands
- Apply flow that computes a full-reset desired-state map

### Out of scope

- Changes to `scanner`, `planner`, `applier`, or `rule_sync` internals
- New settings keys (suite storage path is fixed in v1)

## Existing system and reuse

| Existing component | What it does | How this module reuses it |
|---|---|---|
| `scanner` | Scans shared root, produces `CapabilityItem[]` | Manager Suite scope reads the global scan for the capability checklist; suite apply validates references |
| `planner::inspect` | Current per-tool state per item | Suite apply calls this before building plan |
| `planner::build_plan` | Plan ops from desired-state map | Suite apply provides full-coverage desired map |
| `applier::apply` | Executes operations safely | Suite apply calls this with the generated plan |
| `rule_sync::sync_markdown_rules` | Managed instruction block | Suite apply calls this after projection changes |
| `adapter_registry` | Tool adapters | Suite apply uses adapters for state inspection |
| `settings` | Reads tool settings | Suite apply reads settings to resolve adapters |

## Design principles

| Principle | Meaning in this module |
|-----------|------------------------|
| Reuse over fork | Suite apply constructs the same `desired_enabled_by_item_id` map the existing apply expects. No parallel apply pipeline. |
| Filesystem truth | Suite definitions live on disk in a dotfile. No shadow state. |
| Explicit reset | Apply always means full reset. No merge, no partial apply. The user knows exactly what will happen. |
| Free editing after apply | Suites set the starting point. Individual toggles work normally after. |

## Architecture

```
+-------------------------------+      +--------------------------------+
| Manager rail                  |      | CapabilityTable                |
| Global / Suites / Workspaces  |----->| caller-defined state columns   |
| suite id selection            |      | Suite: Included tri-state      |
+--------------+----------------+      +---------------+----------------+
               |                                       |
               v                                       v
       +-------+-------+                     +---------+----------+
       | suites store  |                     | apply_suite handler |
       | draft + CRUD  |                     | existing pipeline   |
       +-------+-------+                     +--------------------+
               |
               v
   ~/.agentic-suites.json
```

The module adds a storage layer and wires it into the existing pipeline. No new projection logic.

## Data model

### SuiteDefinition

```rust
pub struct SuiteDefinition {
    pub id: String,                          // UUID v4
    pub name: String,                        // unique, human-readable
    pub description: Option<String>,
    pub capabilities: Vec<SuiteCapabilityRef>, // source-qualified refs
    pub is_base: bool,                       // single base suite (#[serde(default)] = false)
    pub created_at: String,                  // ISO 8601
    pub updated_at: String,
}

pub struct SuiteCapabilityRef {
    pub cap: String,                  // bare capability id, e.g. skill:dev/tdd
    pub source: Option<SourceRef>,    // portable source identity (None = legacy/unqualified)
}
```

The bare `cap` uses the same format as `CapabilityItem.id` produced by `scanner` — source-free (e.g. `skill:dev/tdd`). The optional `source` ([`SourceRef`](./multi-source-roots.md#portable-source-identity-cross-device)) makes a reference portable across devices.

### Source-qualified entries (cross-device portability)

Suite files sync across machines. Without a source identity, a synced `skill:foo` could silently re-resolve to a *different* source's `skill:foo`, and there was no way to mark "this came from a source that isn't on this device". `SuiteCapabilityRef` fixes both:

- **Match**: a qualified ref resolves only to a scanned item whose `source` matches (by home-relative path, then folder); an unqualified ref matches by bare id alone.
- **Absent source**: a qualified ref whose source is not present on this machine is *skipped and preserved* — its projection cannot exist locally, so it is never deleted, and it never mis-resolves onto a same-named local capability. Counted as `skipped_absent_source`.
- **Migration**: `SuiteCapabilityRef` deserializes tolerantly from a legacy bare string (`"skill:dev/tdd"` → `{ cap, source: None }`) and always serializes as an object, so suite files upgrade in place on the next write. Existing suites need no manual migration.
- **Backfill (in-memory only)**: on apply/update the store qualifies unqualified refs whose bare id resolves to exactly one scanned item (`SuiteStore::backfill_sources`) so matching is source-precise. This upgrade is **never persisted from apply/update** — the UI draft preserves existing refs and records a live item's source when the user explicitly selects it, then Save persists that source-aware draft (`src/state/suites.ts`). Persisting backfill from apply/edit made two synced machines rewrite the file with device-specific source qualifiers at different times; a later `git pull` then line-merged the divergent multi-line `capabilities` arrays into an empty set (suite name survived, resources went empty). As of v0.8.1 apply and palette "Apply suite…" are pure reads of the suites file.

Adding or reordering sources never changes a suite's bare IDs. A present-source (or unqualified) ref that matches no scanned item is reported as stale (see stale reconciliation below).

### Base suite (global merge)

Exactly one suite may be marked **base** (`is_base: true`). Its capabilities are unioned into *every* global apply, so its rules/skills are always present whatever suite a tool runs. The flag is portable (lives in `~/.agentic-suites.json`) and defaults to `false` for legacy files.

- **Single-base invariant**: setting one suite base clears the flag on every other. `SuiteStore::set_base(Some(id))` (or `update` with `is_base: Some(true)`) funnels through this rule; `set_base(None)` clears all.
- **Merge semantics**: `api::merge_base_caps(selected, base)` clones the selected suite and appends the base's capabilities, deduped by `(cap, source)`. It keeps the **selected** suite's identity, so `ApplySuiteResult.suite` and the recorded binding still point at the explicitly chosen suite — the base is invisible to binding bookkeeping. A `None` base, or a base whose id equals the selected suite, is a no-op.
- **Where it merges**: every global apply path — the Manager Suites scope apply, the palette suite apply, and bound-tool re-syncs — applies the merged "effective" suite. `apply_suite` itself takes the already-merged suite; it does not know about the base.
- **Re-sync on base change**: setting/unsetting/editing the base re-applies **every** bound tool (each tool's own selected suite re-merged with the new base). Editing a normal suite re-applies only the tools bound to it (still base-merged).

### Dotfile contract

Path: `~/.agentic-suites.json` (preserved from VS Code extension for parity)

```json
{
  "version": 1,
  "suites": [
    {
      "id": "abc123-...",
      "name": "coding-workflow",
      "description": "Full dev setup with all coding skills and agents",
      "capabilities": [
        "skill:code-review",
        "skill:dev/tdd",
        "agent:coding/coding-agent",
        "rule:general/precise",
        "rule:general/workspace",
        "hook:auto-format-after-edit",
        "command:review/code-review.md"
      ],
      "createdAt": "2026-05-20T10:00:00Z",
      "updatedAt": "2026-05-20T10:00:00Z"
    }
  ]
}
```

Conventions:
- File created on first suite save if it does not exist
- Reads tolerant of malformed JSON: surface error, return empty list, do not auto-overwrite
- Writes atomic: `.tmp` then `rename`, preceded by a one-level `<file>.bak` backup of the prior good (non-empty) file (`paths::back_up_dotfile`) so an accidental clobber is recoverable without `git checkout`
- `version` field allows future migration
- Suite name uniqueness enforced at create / rename time

## Why a dotfile, not app config

- VS Code extension already uses this path; preserving it allows users to migrate without re-creating suites
- Portable: easy to back up; can be read by tools outside Agentic Hub
- Lives next to `~/.agentic/` (shared root) without being inside it, keeping the shared root clean

## Components and responsibilities

| Component | Responsibility |
|-----------|----------------|
| `suite_store` (Rust) | CRUD on `~/.agentic-suites.json`; atomic write; UUID generation; uniqueness check; validation against scan |
| `ScopeRail` (React) | Global, suite creation/selection, and workspace navigation; Create from current tool prompt |
| `CapabilityTable` (React) | Shared toolbar, filters, flat/tree hierarchy, capability/source/usage columns, row actions, and caller state columns |
| `SuitesPage` (React) | Suite draft fields/actions and Included column wiring; source-aware missing-reference warning/removal |
| `apply_suite` (Rust command handler) | Orchestrates scan + plan + apply + rule_sync with full-reset map |
| Tauri event `suite-store-changed` | Notifies the Manager rail when suite storage changes |

## Rust API

```rust
pub struct SuiteValidationResult {
    pub valid_ids: Vec<String>,    // refs resolving to a scanned item
    pub stale_ids: Vec<String>,    // present-source/unqualified refs with no match
    pub absent_ids: Vec<String>,   // qualified refs whose source is not present here
}

impl SuiteStore {
    pub fn list(&self) -> Result<Vec<SuiteDefinition>>;
    pub fn get(&self, id: &str) -> Result<Option<SuiteDefinition>>;
    pub fn create(&self, input: SuiteCreateInput) -> Result<SuiteDefinition>;
    pub fn update(&self, id: &str, input: SuiteUpdateInput) -> Result<SuiteDefinition>;
    pub fn remove(&self, id: &str) -> Result<()>;
    pub fn put(&self, suite: &SuiteDefinition) -> Result<()>; // in-place replace (backfill/upgrade)
    pub fn base(&self) -> Result<Option<SuiteDefinition>>;     // the single base suite, if any
    pub fn set_base(&self, id: Option<&str>) -> Result<()>;    // single-base invariant; None clears
    pub fn validate(suite: &SuiteDefinition, items: &[CapabilityItem], sources: &[SourceConfig]) -> SuiteValidationResult;
    pub fn backfill_sources(suite: &mut SuiteDefinition, items: &[CapabilityItem]) -> bool;
}

// Pure helpers (api.rs):
//   merge_base_caps(selected: &SuiteDefinition, base: Option<&SuiteDefinition>) -> SuiteDefinition
//   suite_ownership(items, bindings, suites, base) -> Vec<SuiteOwnership>
// merge_base_caps unions+dedups keeping selected identity; suite_ownership maps
// each managed (tool, item) to its owning suite (from_base marks base-merged items)
// so the Manager can lock the cell and name the owner on hover.
```

## IPC commands

See [tauri-ipc-contract.md](./tauri-ipc-contract.md) for full schemas. Summary:

- `cmd_list_suites() -> Vec<SuiteDefinition>`
- `cmd_get_suite(id) -> Option<SuiteDefinition>`
- `cmd_create_suite(input) -> SuiteDefinition`
- `cmd_update_suite(id, input) -> SuiteDefinition`
- `cmd_delete_suite(id) -> ()`
- `cmd_apply_suite({ tool_id, suite_id }) -> ApplySuiteResult` (merges the base suite into the effective set)
- `cmd_set_base_suite(id: Option<String>) -> ()` (single-base invariant; re-applies every bound tool)
- `cmd_suite_ownership() -> Vec<SuiteOwnership>` (which suite owns each managed `(tool, item)`)
- Tauri event (global): `suite-store-changed`

## Apply flow (detailed)

The `apply_suite` handler:

```rust
async fn apply_suite(tool_id: ToolId, suite_id: String) -> Result<ApplySuiteResult> {
    let suite = suite_store.get(&suite_id)?.ok_or(Err::SuiteNotFound)?;
    let settings = settings.read()?;
    let scan = scanner::scan_all(&settings.sources)?;  // forest; first-source-wins
    let adapters = adapter_registry::resolve_all(&settings.tools);
    let states = planner::inspect_all_tools(&scan.items, &adapters);

    let adapter = adapters.iter().find(|a| a.tool_id == tool_id)
        .ok_or(Err::ToolUnknown)?;
    if !adapter.enabled { return Err(Err::ToolDisabled); }

    // Full reset: every scanned item gets a desired state, computed source-aware
    // (a qualified ref must match the item's source; an unqualified ref matches
    // by bare id). See SuiteCapabilityRef::matches_item.
    let mut desired: HashMap<String, bool> = HashMap::new();
    for item in &scan.items {
        desired.insert(item.id.clone(), suite.capabilities.iter().any(|r| r.matches_item(item)));
    }

    let items_by_id: HashMap<String, &CapabilityItem> = scan.items.iter().map(|i| (i.id.clone(), i)).collect();
    let states_for_tool: HashMap<String, &ToolCapabilityState> = states.iter()
        .filter(|s| s.tool == tool_id)
        .map(|s| (s.item_id.clone(), s))
        .collect();

    let ops = planner::build_plan(tool_id, &items_by_id, &states_for_tool, &desired);
    let mut result = applier::apply(&ops, &items_by_id)?;

    if adapter.rule_projection == RuleProjectionKind::MarkdownSectionSync {
        let post_apply_states: Vec<ToolCapabilityState> = states.iter()
            .map(|s| if s.tool != tool_id { s.clone() }
                     else { ToolCapabilityState {
                         state: if *desired.get(&s.item_id).unwrap_or(&false) { Enabled } else { Disabled },
                         ..s.clone()
                     }})
            .collect();
        let _ = rule_sync::sync_markdown_rules(RuleSyncInput {
            adapter,
            items: &scan.items,
            states: &post_apply_states,
        })?;
    }

    // Refs that match no scanned item split into absent-source (qualified to a
    // source not present here; preserved) vs stale (present-source/unqualified).
    let local = settings.resolve_sources();
    let (mut stale, mut absent) = (0u32, 0u32);
    for r in &suite.capabilities {
        if scan.items.iter().any(|i| r.matches_item(i)) { continue; }
        match &r.source {
            Some(s) if !source_present(&local, s) => absent += 1,
            _ => stale += 1,
        }
    }

    Ok(ApplySuiteResult { apply_result: result, skipped_stale: stale, skipped_absent_source: absent, suite: suite.into() })
}
```

The key insight: `desired` is built for **every** scanned item, not just those in the suite. Items not in the suite get `false`. This produces a full reset through the existing pipeline.

## Store and navigation coordination

When Manager Suite scope creates, edits, or deletes a suite, it emits a global Tauri event:

```rust
window.emit_all("suite-store-changed", &SuiteStoreChangedEvent {
    kind: "created" | "updated" | "deleted",
    suite_id: ...,
})?;
```

The Manager rail listens for this event and reloads the suite list. Entering
Suite scope first ensures the editable global scan is loaded, so a transition
from Workspace never renders or saves against read-only workspace inventory.
`#/suites` remains accepted and resolves to the Manager route with
`scope = suite`; the palette's Open Suites command continues emitting that
compatibility route.

## Failure modes

| Failure | Impact | Detection | Recovery |
|---------|--------|-----------|----------|
| Dotfile missing | No suites available | `suite_store.list()` returns empty + creates on first save | Auto-create empty file on write |
| Dotfile malformed JSON | List unavailable | `serde_json::from_str` error | Surface error in UI; preserve in-memory state |
| Dotfile write permission denied | Save fails | `fs::rename` error | Surface error; preserve in-memory state |
| Suite references stale capabilities | Some items skipped on apply | Validation during apply | Report `skipped_stale` count in `ApplySuiteResult` |
| Synced suite references an absent source | Those refs skipped, projections preserved | Source-presence check during apply | Report `skipped_absent_source`; never deletes the absent source's projections |
| Concurrent writes | Last writer wins | Atomic rename | Acceptable in v1 (single user) |
| Apply with empty capabilities | All tool capabilities disabled | Valid operation | Confirmation dialog warns explicitly |
| Tool disabled during apply | Apply blocked | Adapter check | Show error; no partial apply |
| Suite name collision at create | Refused | Pre-write check in `suite_store.create` | UI shows error inline |

## Testing strategy

### Unit

- `suite_store`: create / read / update / delete on a tempdir-backed dotfile
- `suite_store`: validation against mock scan results (stale detection)
- `suite_store`: handles missing file / malformed file / permission errors
- Apply flow: desired-state map computation from a suite (all items covered, correct true/false)

### Integration

- Suite apply against a temp shared root + temp tool directories
- Full reset: pre-existing enabled items get disabled; suite items get enabled
- Markdown section sync runs correctly after suite apply
- Stale references produce correct skip count
- Suite-store event broadcast verified via the Tauri event boundary

## Implementation roadmap

| Phase | Deliverable |
|-------|-------------|
| Phase 1 | Types + `suite_store` (CRUD + validation) with unit tests |
| Phase 2 | Manager Suite scope (rail, shared-table editor, create/edit/delete) + IPC commands |
| Phase 3 | Suite action bar (tool selector, apply button, confirmation, full-reset apply flow) |
| Phase 4 | Polish: stale warnings, skipped-stale count in result, "Create from current" shortcut |

## Open questions

- Should suite IDs be UUIDs or slugified names? Decision: UUIDs (stable under rename, matches VS Code extension behavior)
- Should Suite scope run a separate scan? Decision: no; it explicitly restores
  the Manager's global scan before entering Suite scope and reuses that typed
  inventory.
- Should we eventually add a "duplicate suite" affordance? Yes, in Phase 4 polish — single button in the editor
