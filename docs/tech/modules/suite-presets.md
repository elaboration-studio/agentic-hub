# Module: Suite Presets

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/features/suite-presets.md](../../features/suite-presets.md)
Related Docs: [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/modules/workspace-patch.md](./workspace-patch.md)

## Overview

The Suite Presets module adds named capability presets to Agentic Hub. A suite is a tool-agnostic list of capability IDs. Applying a suite to a focused tool fully resets that tool's enabled capabilities to match the suite definition, then runs the existing projection pipeline.

The module reuses the existing scan, plan, apply, and rule-sync code. It adds one new core module (`suite_store`), one new Tauri window (`SuiteManagerPanel`), and integration points in the main window.

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
- Suite Manager Tauri window
- Main window changes for suite selection and apply
- New IPC commands
- Apply flow that computes a full-reset desired-state map

### Out of scope

- Changes to `scanner`, `planner`, `applier`, or `rule_sync` internals
- New settings keys (suite storage path is fixed in v1)

## Existing system and reuse

| Existing component | What it does | How this module reuses it |
|---|---|---|
| `scanner` | Scans shared root, produces `CapabilityItem[]` | Suite Manager reads scan results for the capability checklist; suite apply validates references |
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
| Suite Manager Window          |      | Main Window (Capability Mgr)   |
| - List suites                 |      | - Suite selector dropdown      |
| - Edit / create / delete      |      | - Apply Suite button           |
+--------------+----------------+      +--------------+-----------------+
               |                                      |
               v                                      v
       +-------+-------+                  +-----------+-----------+
       |  suite_store  | <--------------- | apply_suite handler   |
       |  (CRUD,       |                  | (full-reset desired   |
       |   validate)   |                  |  map -> existing      |
       +-------+-------+                  |  plan/apply pipeline) |
               |                          +-----------+-----------+
               v                                      |
   ~/.agentic-suites.json                             v
                                          (existing plan/apply/sync)
```

The module adds a storage layer and wires it into the existing pipeline. No new projection logic.

## Data model

### SuiteDefinition

```rust
pub struct SuiteDefinition {
    pub id: String,                   // UUID v4
    pub name: String,                 // unique, human-readable
    pub description: Option<String>,
    pub capabilities: Vec<String>,    // capability IDs from scanner
    pub created_at: String,           // ISO 8601
    pub updated_at: String,
}
```

Capability IDs use the same format as `CapabilityItem.id` produced by `scanner`. Stable as long as the capability's relative path in the shared root does not change.

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
        "rule:general/workspace"
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
- Writes atomic: `.tmp` then `rename`
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
| `SuiteManagerPanel` (React) | List + editor UI; reads scan data for checklist; sends CRUD IPC |
| `CapabilityManagerPanel` (React, updated) | Suite selector dropdown; "Apply Suite" button; full-reset desired-map computation |
| `apply_suite` (Rust command handler) | Orchestrates scan + plan + apply + rule_sync with full-reset map |
| Tauri event `suite-store-changed` | Notifies main window when Suite Manager mutates the store |

## Rust API

```rust
pub struct SuiteValidationResult {
    pub valid_ids: Vec<String>,    // capability IDs present in scan
    pub stale_ids: Vec<String>,    // capability IDs not found in scan
}

impl SuiteStore {
    pub fn list(&self) -> Result<Vec<SuiteDefinition>>;
    pub fn get(&self, id: &str) -> Result<Option<SuiteDefinition>>;
    pub fn create(&self, input: SuiteCreateInput) -> Result<SuiteDefinition>;
    pub fn update(&self, id: &str, input: SuiteUpdateInput) -> Result<SuiteDefinition>;
    pub fn remove(&self, id: &str) -> Result<()>;
    pub fn validate(suite: &SuiteDefinition, items: &[CapabilityItem]) -> SuiteValidationResult;
}
```

## IPC commands

See [tauri-ipc-contract.md](./tauri-ipc-contract.md) for full schemas. Summary:

- `cmd_list_suites() -> Vec<SuiteDefinition>`
- `cmd_get_suite(id) -> Option<SuiteDefinition>`
- `cmd_create_suite(input) -> SuiteDefinition`
- `cmd_update_suite(id, input) -> SuiteDefinition`
- `cmd_delete_suite(id) -> ()`
- `cmd_apply_suite({ tool_id, suite_id }) -> ApplySuiteResult`
- Tauri event (global): `suite-store-changed`

## Apply flow (detailed)

The `apply_suite` handler:

```rust
async fn apply_suite(tool_id: ToolId, suite_id: String) -> Result<ApplySuiteResult> {
    let suite = suite_store.get(&suite_id)?.ok_or(Err::SuiteNotFound)?;
    let settings = settings.read()?;
    let scan = scanner::scan(&settings.shared_root)?;
    let adapters = adapter_registry::resolve_all(&settings.tools);
    let states = planner::inspect_all_tools(&scan.items, &adapters);

    let adapter = adapters.iter().find(|a| a.tool_id == tool_id)
        .ok_or(Err::ToolUnknown)?;
    if !adapter.enabled { return Err(Err::ToolDisabled); }

    let suite_set: HashSet<&str> = suite.capabilities.iter().map(|s| s.as_str()).collect();

    // Full reset: every scanned item gets a desired state
    let mut desired: HashMap<String, bool> = HashMap::new();
    for item in &scan.items {
        desired.insert(item.id.clone(), suite_set.contains(item.id.as_str()));
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

    let stale = suite.capabilities.iter()
        .filter(|cap_id| !scan.items.iter().any(|i| &i.id == *cap_id))
        .count() as u32;

    Ok(ApplySuiteResult { apply_result: result, skipped_stale: stale, suite: suite.into() })
}
```

The key insight: `desired` is built for **every** scanned item, not just those in the suite. Items not in the suite get `false`. This produces a full reset through the existing pipeline.

## Cross-window coordination

When Suite Manager creates / edits / deletes a suite, it emits a global Tauri event:

```rust
window.emit_all("suite-store-changed", &SuiteStoreChangedEvent {
    kind: "created" | "updated" | "deleted",
    suite_id: ...,
})?;
```

The main window listens for this event and refreshes its suite dropdown. The Suite Manager window does the same to handle the edge case of two Suite Manager windows open at once (unlikely but cheap to handle).

## Failure modes

| Failure | Impact | Detection | Recovery |
|---------|--------|-----------|----------|
| Dotfile missing | No suites available | `suite_store.list()` returns empty + creates on first save | Auto-create empty file on write |
| Dotfile malformed JSON | List unavailable | `serde_json::from_str` error | Surface error in UI; preserve in-memory state |
| Dotfile write permission denied | Save fails | `fs::rename` error | Surface error; preserve in-memory state |
| Suite references stale capabilities | Some items skipped on apply | Validation during apply | Report count in `ApplySuiteResult` |
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
- Cross-window event broadcast verified via Tauri test harness (or per-window subscribe + emit-and-poll)

## Implementation roadmap

| Phase | Deliverable |
|-------|-------------|
| Phase 1 | Types + `suite_store` (CRUD + validation) with unit tests |
| Phase 2 | Suite Manager window (list, editor, create/edit/delete) + IPC commands |
| Phase 3 | Main window integration (selector, apply button, confirmation, full-reset apply flow) |
| Phase 4 | Polish: stale warnings, skipped-stale count in result, "Create from current" shortcut |

## Open questions

- Should suite IDs be UUIDs or slugified names? Decision: UUIDs (stable under rename, matches VS Code extension behavior)
- Should the Suite Manager share the main window's scan cache or run its own scan? Decision: run its own scan on open; scans are fast enough and isolation is simpler
- Should we eventually add a "duplicate suite" affordance? Yes, in Phase 4 polish — single button in the editor
