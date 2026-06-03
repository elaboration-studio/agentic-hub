# Module: Multi-Source Capability Roots

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-31
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/tech/reference/shared-root-contract.md](../reference/shared-root-contract.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../../features/mvp-unified-agentic-capability-manager.md), [docs/tech/modules/claude-flat-skill-layout.md](./claude-flat-skill-layout.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md), [docs/tech/modules/suite-presets.md](./suite-presets.md)

## Why this exists

The manager started with a single `shared_root` setting. Real users keep capabilities in more than one place — a personal `~/.agentic-arno`, a synced team folder, an open-source skills library cloned from GitHub. A single root forces them to symlink or merge things by hand.

Multi-source roots make this first-class: an ordered list of source folders, each contributing its own root node in the inventory tree. Search, kind filter, suites, and apply continue to operate across the entire forest.

## Contract

A **source** is the smallest unit of capability ownership:

```rust
pub struct SourceConfig {
    pub id: String,    // stable slug derived from the label (deterministic de-dup)
    pub label: String, // shown as the tree root and in messages
    pub path: PathBuf, // absolute, normalized
}
```

Sources are persisted under `sources` in `~/.agentic-hub/config.json` as `{ label, path }` pairs. A pure helper derives a stable `id` slug from each label so downstream code routes messages without depending on array index:

```rust
pub fn resolve_source_configs(
    raw: &[RawSource],          // persisted { label, path }
    legacy_shared_root: Option<&Path>,
    normalize: impl Fn(&str) -> PathBuf,
) -> Vec<SourceConfig>;
```

```jsonc
// ~/.agentic-hub/config.json
"sources": [
  { "label": "Arno", "path": "~/.agentic-arno" },
  { "label": "Team", "path": "~/work/team-agentic" }
]
```

When `sources` is empty, `resolve_source_configs` synthesizes a single `Default` source from the legacy `shared_root` setting. This keeps existing installs working through one release; `shared_root` is marked deprecated.

This is the Rust port of the extension's `agenticSourceConfig.resolveSourceConfigs`; the slug + legacy-fallback + de-dup behavior is preserved verbatim.

## Order is meaningful

The list order encodes priority. Two collision flavors use it:

1. **Scan-time collisions.** Two sources may both contain `skills/dev/tdd/SKILL.md`. The scanner keys items by `${kind}:${relative_path}` and keeps only the **first source's** item. The shadowed item is dropped from inventory and surfaced as a `ScanError` so the user can rename or reorder.

2. **Projection-target collisions.** Two enabled items (from different sources, or two same-basename items under a `flat` layout like Claude) may resolve to the same target path. The planner sorts enabled items by source order, lets the first source own the target, and emits a `skip_conflict` for the loser whose reason names the winning source:

   > Target `~/.claude/skills/repo-research` is already claimed by skill "dev/repo-research" from source "Arno". Disable the duplicate, reorder sources, or rename one to disambiguate.

The same code path subsumes the original Claude-flat basename-collision case, so that behavior is preserved.

## Scanner

`scanner::scan_all(sources: &[SourceConfig]) -> ScanResult` becomes the primary entry point:

```
items_by_key: Map<String, CapabilityItem> = {}
errors: Vec<ScanError> = []
for source in sources (priority order):
    if !source.path.is_dir():
        errors.push(ScanError { path: source.path, message: "source folder missing or not a directory" })
        continue
    for item in walk(source.path):           // skills/agents/rules/hooks, __archived__ skipped
        item.source_id = source.id
        item.source_label = source.label
        key = format!("{}:{}", item.kind, item.relative_path)
        if items_by_key.contains(key):
            errors.push(ScanError { path: item.source_path,
                message: format!("shadowed by higher-priority source; \"{}\" already provides {}", winner.source_label, key) })
        else:
            items_by_key.insert(key, item)
return ScanResult { items: items_by_key.values(), errors }
```

`scanner::scan(shared_root)` is retained as a thin wrapper over `scan_all(&[Default-source])` so external callers keep compiling. Internal callers use `scan_all(settings.sources)`.

`CapabilityItem` gains required `source_id` / `source_label`, surfaced in the inspector's **Source** row.

## Planner

Generalize the existing flat-layout collision pass (see [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)) into a **projection-target collision pass** that covers all cross-source target clashes, not just Claude flat-layout basenames:

```
group enabled plan ops by (tool, target_path)
for each group with len > 1:
    sort by source priority (source order), then item_id for determinism
    first wins -> keep its create/replace op
    others    -> rewrite to SkipConflict, reason names the winning source + item
```

The Claude-flat basename case is one instance of this general pass.

## Identity strategy (keep flat)

Capability IDs stay source-free — `skill:dev/tdd`, `rule:general/precise.mdc`, `hook:auto-format-after-edit`. They intentionally do **not** include the source slug. Two consequences, both intended:

- Existing `~/.agentic-suites.json` suites keep working with **no migration**. A suite ID resolves to whichever source currently owns it (first wins).
- Adding or reordering sources never changes a capability's ID; only which source backs it can change.

The scan-time dedupe error gives the user enough signal to fix shadowing manually, without rewriting every suite.

## Portable source identity (cross-device)

Suite files (`~/.agentic-suites.json`) and bindings sync across machines, but absolute paths and slug IDs are device-specific. To keep a synced suite resolving to the *right* source — and to avoid mis-resolving a synced `skill:foo` onto a different source's same-named `skill:foo` — every scanned item carries a portable `SourceRef`:

```rust
pub struct SourceRef {
    pub rel_home: String, // home-relative path (~/.agentic), or absolute if outside ~
    pub folder: String,   // last path component (.agentic)
}
```

`SourceConfig::portable_ref()` computes it from the resolved (tilde-expanded) path. Two devices match "the same" logical source by `rel_home` first, then `folder` (`SourceRef::matches`). `settings::source_present` answers whether a synced ref's source exists locally.

Capability IDs stay bare (`kind:rel`) everywhere — scanner keys, managed-copy manifest `itemId`, and the Matrix UI are untouched. The `SourceRef` rides alongside as a separate identity, used only by suite resolution. See [suite-presets.md](./suite-presets.md) for how suites qualify their entries and skip (never delete) capabilities whose source is absent on the current machine.

## Tree shape

```text
[Arno]                <- source root, label from SourceConfig
  Skills (N visible)
  Agents
  Rules
  Hooks
[Team]
  Skills
  ...
```

Search collapses the forest back to a single synthetic "All results" root so users can scan matches without bucketing. The kind filter, indeterminate parent checkboxes, bulk-toggle, and Expand All work unchanged because they always operate on `node.item_ids`.

The inspector for an item shows a **Source** row. The inspector for a source root shows the source's path plus a **Remove Source** button.

## IPC surface

See [tauri-ipc-contract.md](./tauri-ipc-contract.md) for full schemas. The deltas:

- `cmd_scan(input: { sources: SourceConfig[] }) -> ScanResult` — multi-source entry; the legacy single-root call is kept as a wrapper.
- `cmd_add_source(input: { label: string, path: string }) -> Settings` — the shell opens a Tauri folder dialog before the call; validates uniqueness of label and normalized path; appends to `settings.sources`; emits `settings-changed`.
- `cmd_remove_source(input: { id: string }) -> Settings` — splices the entry out of `settings.sources`; never touches disk. Removing the only source falls back to the legacy `shared_root` Default.
- `CapabilityItem` gains `sourceId` / `sourceLabel`.
- New error code `source_path_invalid`.
- `Settings` gains `sources: SourceConfig[]`; `sharedRoot` becomes deprecated (one-release fallback).

## Add / remove source UX

`Add Source…` (header button):

1. Open the Tauri folder dialog for a directory.
2. Prompt for a label (default: folder basename).
3. Validate uniqueness of label and normalized path.
4. `cmd_add_source` persists and the panel refreshes.

`Remove Source` (per-source inspector) is modal-confirmed; `cmd_remove_source` splices the entry and refreshes. Files on disk are never touched. The demo scaffold writes into `sources[0].path` (falling back to `shared_root` when no sources are configured).

## Backward compatibility

- `shared_root` is retained for one release. When `sources` is empty it is treated as a single `Default` source.
- `scanner::scan(shared_root)` remains a thin wrapper over `scan_all`.
- Suites need no migration (flat IDs).

## Tests

- `scanner` multi-source merge: first-source-wins dedupe by `kind:relative_path`; shadow reported as `ScanError`; missing-source-folder reported; empty-sources legacy fallback.
- `resolve_source_configs`: legacy fallback, invalid entries, slug de-dup determinism.
- `planner`: source-priority projection-target collision (first source wins, loser → `skip_conflict`); existing Claude flat-layout collision still passes.

## Future extensions

- Per-source enable/disable toggle (currently every configured source is scanned).
- Drag-reorder UI for source priority.
- Workspace-scoped sources, layered on top of the global-only configuration.
