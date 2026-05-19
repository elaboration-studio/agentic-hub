# Architecture: Projection Engine

This document deepens the projection engine design for Agentic Hub. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for the full system view.

## Context from root architecture

The root architecture establishes that the Rust core (`agentic-core`) owns all filesystem mutation, and that the projection engine — scan → plan → apply → rule-sync — is the bulk of the system. This doc owns the engine: how each pipeline stage works, the per-tool layout matrix, the Cursor managed-copy lifecycle, and the Claude flat-layout collision contract.

## Why this domain is split out

The projection engine is the single largest piece of architecturally novel design in this app. It has:

- four pipeline stages, each with its own data model and failure modes
- four tool adapters with three different rule projection modes (`link_sync`, `file_sync`, `markdown_section_sync`)
- two layout strategies (`flat`, `nested`) with explicit collision handling
- a separate managed-copy lifecycle with stale detection for Cursor agents
- a marker-delimited managed block contract for `markdown_section_sync` tools

Folding all of that into the root architecture would either bloat the root doc beyond its 500-line target or under-specify the engine. The engine is also the surface most likely to need iteration as new tools are added, so it benefits from being a separate reading path.

## Goals

- Specify the data flow from scan to apply, including state transitions per item
- Specify the per-tool projection matrix (layout × kind × target path × projection mode)
- Specify the Cursor managed-copy lifecycle (create, stale detection, refresh, remove)
- Specify the Claude flat-layout collision contract and basename normalization
- Specify the marker-delimited managed-block contract for rule sync
- Specify how failures at each stage surface in `ApplyResult`

## Non-goals

- Tauri IPC payload schemas (see [docs/tech/modules/tauri-ipc-contract.md](docs/tech/modules/tauri-ipc-contract.md))
- Workspace-scope projection (see [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md))
- Suite apply orchestration (see [docs/tech/modules/suite-presets.md](docs/tech/modules/suite-presets.md))
- UI presentation of states and operations (see [docs/features/mvp-unified-agentic-capability-manager.md](docs/features/mvp-unified-agentic-capability-manager.md))

## Scope and boundaries

In scope:
- `scanner`, `adapter_registry`, `planner`, `applier`, `rule_sync` modules in `agentic-core`
- Global-scope projections only (workspace scope is the sibling doc)
- All four tools (Codex, Claude Code, Cursor, OpenClaw)
- All three projection modes (`link_sync`, `file_sync`, `markdown_section_sync`)
- All four state classes (item state, link state, planned op, apply result)

Out of scope:
- Workspace-scope hard-copy semantics
- Suite apply (consumer of the engine, not part of it)
- UI components

## Existing system and reuse

This engine is a 1:1 port of the VS Code extension's services:

- `AgenticScanService` → `scanner`
- `ToolAdapterRegistry` → `adapter_registry`
- `SymlinkPlanService` → `planner`
- `SymlinkApplyService` → `applier`
- `RuleInstructionSyncService` → `rule_sync`

The behavior is preserved verbatim. The implementation is Rust, the boundary semantics (safe symlinks, collision detection, marker block) are identical.

## Pipeline overview

```
+----------+      +----------+      +---------+      +---------+      +----------+
| settings | ---> | scanner  | ---> | adapter | ---> | planner | ---> | applier  |
+----------+      +----------+      | registry|      +---------+      +----------+
                  scan items        +---------+      plan ops         executed ops
                                    target paths
                                                                            |
                                                                            v
                                                                      +----------+
                                                                      | rule_sync|
                                                                      +----------+
                                                                      managed
                                                                      markdown
                                                                      blocks
```

Each stage is a pure function over its inputs except `applier` and `rule_sync`, which mutate disk. Plan output is the only stage that ever needs to be shown to the user before any FS write.

## Stage 1: Scan

Input: `shared_root: PathBuf`
Output: `ScanResult { items: Vec<CapabilityItem>, errors: Vec<ScanError> }`

Walk strategy:

```
<shared_root>/skills/**/SKILL.md           -> CapabilityKind::Skill
<shared_root>/agents/**/*.{md}              -> CapabilityKind::Agent
<shared_root>/rules/**/*.{md,mdc}           -> CapabilityKind::Rule
```

Validation per kind:

- **Skill**: parent directory of `SKILL.md` exists; `SKILL.md` is a regular file (after symlink resolution). The capability's `source_path` is the parent directory.
- **Agent**: file matches `*.md`. The capability's `source_path` is the file.
- **Rule**: file matches `*.md` or `*.mdc`. The capability's `source_path` is the file.

The `relative_path` is the path under `<shared_root>/<kind-dir>/`. Examples:

| Source | Kind | `relative_path` |
|--------|------|-----------------|
| `~/.agentic/skills/skill-a/SKILL.md` | Skill | `skill-a` |
| `~/.agentic/skills/dev/repo-research/SKILL.md` | Skill | `dev/repo-research` |
| `~/.agentic/agents/foo.md` | Agent | `foo.md` |
| `~/.agentic/agents/group/bar.md` | Agent | `group/bar.md` |
| `~/.agentic/rules/general/precise.mdc` | Rule | `general/precise.mdc` |

The `id` is `<kind>:<relative_path_without_extension_for_skills>`. Stable as long as the source location does not change.

Scan does **not** follow symlinks outside the shared root. A symlink inside `~/.agentic/skills/` pointing to `~/.agentic-arno/skills/foo/` is followed; a symlink pointing to `/tmp/` is followed but logged.

## Stage 2: Adapter resolution

Input: `ToolsSettings`
Output: `Vec<ResolvedAdapter>`

Each adapter resolves:

- `tool_id`
- `enabled` (from settings)
- `skills_path`, `agents_path`, `rules_path` (tilde-expanded, canonicalized)
- `instructions_path` (canonicalized) for `markdown_section_sync` tools
- `skill_layout`, `agent_layout` (intrinsic per tool)
- `rule_projection` (intrinsic per tool, configurable in settings)

The adapter is the **only** code that decides basename-vs-relative-path:

```rust
impl ResolvedAdapter {
    pub fn target_path_for(&self, item: &CapabilityItem) -> PathBuf {
        let base = match item.kind {
            CapabilityKind::Skill => &self.skills_path,
            CapabilityKind::Agent => &self.agents_path,
            CapabilityKind::Rule => &self.rules_path,
        };
        let layout = match item.kind {
            CapabilityKind::Skill => self.skill_layout,
            CapabilityKind::Agent => self.agent_layout,
            CapabilityKind::Rule => Layout::Nested,
        };
        let rel = match layout {
            Layout::Nested => item.relative_path.clone(),
            Layout::Flat => PathBuf::from(item.relative_path.file_name().unwrap()),
        };
        base.join(rel)
    }
}
```

The full per-tool matrix is in [docs/tech/reference/tool-adapter-matrix.md](docs/tech/reference/tool-adapter-matrix.md).

If a tool's resolved path does not exist or is not a directory (when it should be one), the adapter is marked `unavailable` and produces no plan operations.

## Stage 3: Inspect current state

Input: `items`, `adapter`
Output: `Vec<ToolCapabilityState>`

For each (tool, item), compute the current `LinkState`:

```rust
pub enum LinkState {
    Enabled,        // target is a symlink to the correct source (or managed copy with matching metadata)
    Disabled,       // target does not exist
    Broken,         // target is a symlink to a non-existent path
    Stale,          // target is a managed copy whose metadata mismatches the shared source
    ForeignFile,    // target is a real file (not a symlink, not a managed copy with matching metadata)
    ForeignLink,    // target is a symlink to a different source (or managed copy attributed to a different source)
}
```

Algorithm (per (tool, item)):

```
target = adapter.target_path_for(item)
lstat = fs::symlink_metadata(target).ok()

match lstat:
    None                                    -> Disabled
    Some(meta) if meta.is_symlink():
        resolved = fs::read_link(target)
        if resolved == item.source_path     -> Enabled
        elif resolved does not exist        -> Broken
        else                                -> ForeignLink
    Some(meta) if uses_managed_copy(adapter, item.kind):
        if target has e-studio sync metadata file pointing at item.source_path:
            if content hash matches          -> Enabled
            else                             -> Stale
        else:
            if target is a regular file      -> ForeignFile
            else                             -> ForeignFile (or directory; both treated as conflict)
    Some(meta) if meta.is_file()             -> ForeignFile
    Some(meta) if meta.is_dir()              -> ForeignFile  (treated as conflict)
```

`uses_managed_copy(adapter, kind)` returns true for Cursor agents in global scope (and various combos in workspace scope; see sibling doc).

## Stage 4: Plan

Input: `tool_id`, `items_by_id`, `current_states`, `desired_enabled_by_item_id: Map<String, bool>`
Output: `Vec<PlannedOperation>`

For each item in `items_by_id`:

```
state = current_states[(tool, item)]
desired = desired_enabled_by_item_id.get(item.id).unwrap_or(false)

match (state, desired):
    (Enabled,     true)  -> no-op (skip)
    (Enabled,     false) -> RemoveLink or RemoveManagedCopy
    (Disabled,    true)  -> CreateLink or CreateManagedCopy
    (Disabled,    false) -> no-op (skip)
    (Broken,      true)  -> ReplaceLink
    (Broken,      false) -> RemoveLink
    (Stale,       true)  -> ReplaceManagedCopy (refresh from source)
    (Stale,       false) -> RemoveManagedCopy
    (ForeignLink, true)  -> ReplaceLink
    (ForeignLink, false) -> SkipConflict (do not auto-remove other tools' links)
    (ForeignFile, true)  -> SkipConflict (real file blocks projection)
    (ForeignFile, false) -> no-op (real file already not ours)
```

After the per-item pass, the planner runs the **flat-layout collision pass** (see Claude Flat Layout below).

## Stage 5: Apply

Input: `Vec<PlannedOperation>`
Output: `ApplyResult`

For each operation, execute and record outcome:

```rust
match op.kind {
    CreateLink => {
        ensure_parent_exists(op.target_path)?;
        fs::symlink(op.source_path.unwrap(), op.target_path)?;
    }
    ReplaceLink => {
        if !is_symlink_or_managed_copy(op.target_path) { return Err(real_file_block); }
        fs::remove_file(op.target_path)?;
        fs::symlink(op.source_path.unwrap(), op.target_path)?;
    }
    RemoveLink => {
        if !is_symlink(op.target_path) { return Err(real_file_block); }
        fs::remove_file(op.target_path)?;
    }
    CreateManagedCopy => {
        ensure_parent_exists(op.target_path)?;
        copy_with_metadata(op.source_path.unwrap(), op.target_path)?;
    }
    ReplaceManagedCopy => {
        // overwrite via .tmp + rename for atomicity
        copy_with_metadata_atomic(op.source_path.unwrap(), op.target_path)?;
    }
    RemoveManagedCopy => {
        if !is_managed_copy_for(op.target_path, op.source_path) { return Err(real_file_block); }
        fs::remove_file(op.target_path)?;
    }
    SkipConflict => {
        // record reason; no FS op
    }
}
```

The `ApplyResult` aggregates:

```rust
pub struct ApplyResult {
    pub created: u32,
    pub removed: u32,
    pub replaced: u32,
    pub refreshed: u32,
    pub skipped: u32,
    pub errors: Vec<ApplyError>,
}
```

Per-op errors do not abort the apply. The pipeline is partial-apply-tolerant by design.

A progress event is emitted after each op via Tauri's event channel so the UI can show real-time progress for large applies.

## Stage 6: Rule sync (for `markdown_section_sync` tools)

Input: `tool_id`, `items`, `states`, `tool_settings`
Output: `Vec<RuleSyncError>`

Algorithm:

```
enabled_rules = items.filter(kind=Rule).filter(state[(tool, item)] == Enabled or in desired)
target = tool.instructions_path

if target does not exist:
    write_managed_block(target, enabled_rules, mode=create)
    return

content = fs::read_to_string(target)?
case content:
    contains BOTH "<!-- e-studio-agentic-rules:start -->" AND "<!-- ...:end -->":
        if start < end:
            replace block contents with new managed block
        else:
            return RuleSyncError::MalformedMarkers
    contains NEITHER marker:
        append managed block after existing content
    contains exactly ONE marker:
        return RuleSyncError::MalformedMarkers

if enabled_rules is empty AND content outside markers is empty:
    fs::remove_file(target)
elif enabled_rules is empty:
    remove only the managed block
```

The managed block format is documented in [docs/tech/modules/rule-projection-sync.md](docs/tech/modules/rule-projection-sync.md).

## Claude flat-layout collision contract

Claude Code's loader scans only the top level of `~/.claude/skills/` and `~/.claude/agents/`. Nested folders are treated as opaque single entries. Two source items with the same basename project to the same flat target — a silent overwrite risk.

The planner's flat-layout collision pass:

```
collisions = group plan ops by (tool, target_path) where tool.layout == Flat
for each group with len > 1:
    sort by item_id (lexicographic, deterministic)
    first wins -> keep its CreateLink/ReplaceLink op as-is
    others    -> rewrite op to SkipConflict with reason:
                "Basename collision in flat layout: <other.relative_path> would
                 overwrite <winner.relative_path> at <target>. Rename the source
                 item to disambiguate."
```

The collision pass runs **after** the per-item pass so it operates on a complete picture.

Inspect-time behavior: when both colliding items resolve to the same target path during state inspection, whichever source the existing symlink points at sees `state: Enabled`; the other sees `state: ForeignLink`. This surfaces the collision in the inventory before the user attempts to apply.

Apply safety: the apply still refuses to remove a real (non-symlink) directory or overwrite a real file. The flat layout does not relax those guards.

User-facing fix: rename a source item under `~/.agentic/`. Do not invent ad-hoc Claude-only nesting.

## Cursor managed-copy lifecycle

Cursor's runtime loads agent files into memory at launch. Symlinks are unreliable because Cursor may follow them once at launch and not re-check. The solution: write the file contents as a real file, with a sidecar metadata block that ties the copy back to the shared source.

Lifecycle:

```
Create:
  target = adapter.target_path_for(item)
  fs::copy(item.source_path, target)
  fs::write(target.with_extension("e-studio-meta.json"), {
      source_path: item.source_path,
      source_hash: sha256(item.source_path contents),
      synced_at: ISO 8601,
  })

State inspection:
  if target exists AND meta.json exists AND meta.source_path == item.source_path:
      if sha256(target contents) == meta.source_hash:
          state = Enabled
      else:
          state = Stale (target was edited by user, drifted from shared source)
  if target exists AND meta.json missing:
      state = ForeignFile

Refresh (Replace):
  fs::copy(item.source_path, target.tmp)
  fs::rename(target.tmp, target)  # atomic
  rewrite meta.json with new hash + timestamp

Remove:
  fs::remove_file(target)
  fs::remove_file(target.with_extension("e-studio-meta.json"))
```

Stale detection vs ForeignFile: managed-copy state is **Stale** only when the metadata sidecar exists and attributes the file to the same shared source but content has diverged. Without the sidecar, the file is treated as user-owned (`ForeignFile`) and apply refuses to overwrite.

The metadata file (`<target>.e-studio-meta.json`) sits adjacent to the managed copy. The `e-studio-` prefix is preserved from the VS Code extension for migration parity. Cursor ignores files with that suffix because they do not match `*.md`.

## Marker-delimited managed-block contract

For `markdown_section_sync` tools (Codex, Claude, OpenClaw), the rule sync module owns exactly one block in the instruction file:

```md
<!-- e-studio-agentic-rules:start -->
## E-Studio Managed Rules

This section is managed by Agentic Hub. Edit rule selections in the
Agentic Capability Manager instead of editing these blocks by hand.

### general/precise.mdc

Source: `~/.agentic/rules/general/precise.mdc`
Mirrored link: `~/.codex/agentic-rules/general/precise.mdc`

...rule body with YAML frontmatter stripped...
<!-- e-studio-agentic-rules:end -->
```

Rules:

- The block contains all currently enabled rules for the tool, one section per rule
- Each rule's body is inlined after stripping YAML frontmatter (everything between leading `---` lines)
- `Mirrored link:` annotation is included only when a real mirrored file exists at the tool's optional `rulesPath`
- All content outside the markers is preserved on every rewrite
- If no rules are enabled, remove only the managed block (and surrounding whitespace), leaving the rest of the file intact
- Delete the whole file only when the managed block was the entire file contents
- Malformed markers (start without end, or end before start) are reported as `RuleSyncError::MalformedMarkers`; no rewrite

Markers preserved verbatim from VS Code extension. See [PRODUCT.md Open Questions](PRODUCT.md) for rename discussion.

## Components and responsibilities

| Component | Owns |
|-----------|------|
| `scanner` | Shared-root walk, validation per kind, `CapabilityItem` construction |
| `adapter_registry` | Per-tool target path resolution, layout strategy, projection kind |
| `planner::inspect` | Current per-tool state per item |
| `planner::build_plan` | Diff desired vs current; emit operations; collision pass |
| `applier` | Execute operations safely; aggregate `ApplyResult`; emit progress events |
| `rule_sync` | Managed-block contract for `markdown_section_sync` tools |
| `managed_copy` (sub-module of applier) | Cursor-style copy with metadata sidecar |

## Key flows

### Full apply from desired-state map

```
UI -> cmd_apply
agentic-core::apply_for_tool(tool_id, desired):
    1. scanner::scan(settings.shared_root) -> items
    2. adapter = adapter_registry::resolve(settings, tool_id)
    3. states = planner::inspect(items, adapter)
    4. ops = planner::build_plan(items, states, desired)
    5. result = applier::apply(ops)
    6. if adapter.rule_projection == MarkdownSectionSync:
           rule_sync::sync_markdown_rules(adapter, items, post_apply_states)
    7. return result
```

### Refresh-only (no apply)

```
UI -> cmd_inspect
agentic-core::inspect_all_tools():
    1. items = scanner::scan(settings.shared_root)
    2. for each enabled tool:
           adapter = adapter_registry::resolve(settings, tool_id)
           states = planner::inspect(items, adapter)
    3. return items + states_by_tool
```

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| `scanner` | Shared root missing | Empty `items` + scan error | UI shows empty state |
| `scanner` | Permission denied on shared root subdir | Subdir items missing | Surface in `errors`; continue scan |
| `scanner` | Symlink cycle inside shared root | Walk bounded by max depth | Stop at cycle; log warning |
| `adapter_registry` | Tool target path is a file (not dir) | Adapter unavailable | Tool tab disabled; no plan ops |
| `planner::inspect` | Target is on a different filesystem | Symlink read works | No special handling needed |
| `planner::build_plan` | Flat-layout collision | First-id-wins, others `SkipConflict` | Surface in result; user renames source |
| `applier` | `EACCES` on link create | Single op fails | Continue; record error |
| `applier` | `ENOSPC` (disk full) | Multiple ops may fail | Continue best-effort; surface error count |
| `applier::managed_copy` | Source removed between plan and apply | Copy fails | Record error; do not write empty file |
| `rule_sync` | Instruction file is a directory | Conflict | Record error; do not write |
| `rule_sync` | Markers malformed | `MalformedMarkers` | No rewrite; user fixes manually |
| `rule_sync` | Atomic write fails (rename across fs) | Fall back to direct write | Acceptable risk for instruction files |

## Operational rules

- Apply is always idempotent: re-running the same plan against unchanged disk produces zero new operations
- Plan is always re-computed from fresh disk state; never reused across user interactions
- Managed-copy metadata sidecars use the `.e-studio-meta.json` extension to preserve VS Code extension compatibility — do not rename
- Marker-delimited block markers (`e-studio-agentic-rules:start/end`) are preserved verbatim
- Path canonicalization happens once per command; downstream code operates on canonical paths
- Symlink creation is platform-specific; Windows code path is documented in [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) but not exercised in v1

## Related detailed docs

- [docs/tech/modules/rule-projection-sync.md](docs/tech/modules/rule-projection-sync.md) — managed-block format details
- [docs/tech/modules/claude-flat-skill-layout.md](docs/tech/modules/claude-flat-skill-layout.md) — flat layout deep dive
- [docs/tech/modules/openclaw-tool-adapter.md](docs/tech/modules/openclaw-tool-adapter.md) — OpenClaw-specific notes
- [docs/tech/reference/tool-adapter-matrix.md](docs/tech/reference/tool-adapter-matrix.md) — full projection matrix
- [docs/tech/reference/shared-root-contract.md](docs/tech/reference/shared-root-contract.md) — shared root layout contract

## Open questions

- **Per-skill metadata files.** Currently each managed agent copy has a `<file>.e-studio-meta.json` sidecar. Should we move to a single per-tool manifest (`~/.cursor/agents/.agentic-hub-manifest.json`) so the agents directory stays clean? Decision: keep per-file sidecars in v1 for simplicity and crash-tolerance; revisit if user feedback says the noise is unacceptable.
- **Per-rule mirrored files.** The `Mirrored link:` annotation references a real mirrored file at the tool's `rulesPath` if it exists. We do not currently create the mirrored file by default for `markdown_section_sync` tools; the annotation is only added when an external workflow has created one. Decision: keep behavior; document that users who want a real file can configure `rulesPath` and the apply will create the mirror.
- **Concurrent apply across windows.** If the main window and Suite Manager window both trigger apply at the same time, we have a race. Decision: serialize through a Tokio mutex in the Tauri shell layer (one apply at a time, queued).
