# Architecture: Projection Engine

This document deepens the projection engine design for Agentic Hub. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for the full system view.

## Context from root architecture

The root architecture establishes that the Rust core (`agentic-core`) owns all filesystem mutation, and that the projection engine — scan → plan → apply → rule-sync — is the bulk of the system. This doc owns the engine: how each pipeline stage works, the per-tool layout matrix, the Cursor managed-copy lifecycle, and the Claude flat-layout collision contract.

## Why this domain is split out

The projection engine is the single largest piece of architecturally novel design in this app. It has:

- four pipeline stages, each with its own data model and failure modes
- five tool adapters with five projection modes (`link_sync`, `file_sync`, `codex_agent_toml`, `markdown_section_sync`, `json_section`)
- two layout strategies (`flat`, `nested`) with explicit collision handling
- a separate managed-copy lifecycle with stale detection for Cursor agents and Codex TOML subagents
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
- All five tools (Codex, Claude Code, Cursor, OpenClaw, OpenStandard)
- All five projection modes (`link_sync`, `file_sync`, `codex_agent_toml`, `markdown_section_sync`, `json_section`)
- All five capability kinds (`skill`, `agent`, `rule`, `hook`, `command`)
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
- `HookProjectionSyncService` + `HookEventMapper` → `hook_sync`

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

Input: `sources: Vec<SourceConfig>` (ordered by priority; a single legacy `shared_root` resolves to one `Default` source)
Output: `ScanResult { items: Vec<CapabilityItem>, errors: Vec<ScanError> }`

`scanner::scan_all` walks every source in priority order and keys items by `${kind}:${relative_path}`. The first source to provide a key wins; a later source's duplicate is dropped from inventory and surfaced as a `ScanError` (shadowing). Each `CapabilityItem` carries `source_id` / `source_label`. The single-root `scanner::scan(shared_root)` is retained as a thin wrapper. See [docs/tech/modules/multi-source-roots.md](docs/tech/modules/multi-source-roots.md).

Walk strategy (per source):

```
<shared_root>/skills/**/SKILL.md           -> CapabilityKind::Skill
<shared_root>/agents/**/*.{md}              -> CapabilityKind::Agent
<shared_root>/rules/**/*.{md,mdc}           -> CapabilityKind::Rule
<shared_root>/hooks/**/hook.json           -> CapabilityKind::Hook
<shared_root>/commands/**/*.{md}            -> CapabilityKind::Command
```

Any directory named `__archived__` is skipped at every depth. Per the workspace
convention, `__archived__` holds old versions of files; archived capabilities must
never surface in inventory, suites, or plans.

Validation per kind:

- **Skill**: parent directory of `SKILL.md` exists; `SKILL.md` is a regular file (after symlink resolution). The capability's `source_path` is the parent directory.
- **Agent**: file matches `*.md`. The capability's `source_path` is the file.
- **Rule**: file matches `*.md` or `*.mdc`. The capability's `source_path` is the file.
- **Command**: file matches `*.md`. The capability's `source_path` is the file (file-based, nested — same shape as agents).

The `relative_path` is the path under `<shared_root>/<kind-dir>/`. Examples:

| Source | Kind | `relative_path` |
|--------|------|-----------------|
| `~/.agentic/skills/skill-a/SKILL.md` | Skill | `skill-a` |
| `~/.agentic/skills/dev/repo-research/SKILL.md` | Skill | `dev/repo-research` |
| `~/.agentic/agents/foo.md` | Agent | `foo.md` |
| `~/.agentic/agents/group/bar.md` | Agent | `group/bar.md` |
| `~/.agentic/rules/general/precise.mdc` | Rule | `general/precise.mdc` |
| `~/.agentic/commands/review/code-review.md` | Command | `review/code-review.md` |

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
        // Hooks do not project to a per-item path; their target is the tool's
        // single hooks file. The planner routes Hook items to hook_sync, which
        // resolves adapter.hooks_file directly. target_path_for is never called
        // for Hook items.
        let base = match item.kind {
            CapabilityKind::Skill => &self.skills_path,
            CapabilityKind::Agent => &self.agents_path,
            CapabilityKind::Rule => &self.rules_path,
            CapabilityKind::Hook => unreachable!("hooks route through hook_sync, not target_path_for"),
        };
        let layout = match item.kind {
            CapabilityKind::Skill => self.skill_layout,
            CapabilityKind::Agent => self.agent_layout,
            CapabilityKind::Rule => Layout::Nested,
            CapabilityKind::Hook => Layout::Nested,
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

**Confirmed take-over (`force`).** `build_plan` takes a `force` flag (default `false`). When the user explicitly confirms an Apply-time warning, `force` is `true` and `(ForeignFile, true)` becomes a take-over instead of a skip: `ReplaceLink` / `ReplaceManagedCopy` carrying `force: true`. The applier then deletes the blocking real file/dir before projecting. This is the *only* path that removes a real file, and it is always user-initiated and confirmed — the "never overwrite a real file silently" rule still holds. The watcher and suite/workspace applies always pass `force = false`.

After the per-item pass, the planner runs the **projection-target collision pass** — a generalization of the original flat-layout pass. It groups ops by `(tool, target_path)`; for any group with more than one op, items are sorted by source priority (then `item_id` for determinism), the first wins, and the rest become `skip_conflict` with a reason naming the winning source. This covers both cross-source target clashes and the Claude flat-layout basename case (see Claude Flat Layout below).

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
    contains BOTH "<!-- agentic-hub:start -->" AND "<!-- ...:end -->":
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

Claude Code's **skill** loader scans only the top level of `~/.claude/skills/` — nested folders are treated as opaque single entries, so two source skills with the same basename project to the same flat target (a silent overwrite risk). Claude **agents** are scanned recursively (`~/.claude/agents/`, identity from the `name` frontmatter), so the agent layout is `Nested` and this collision pass applies to Claude skills only.

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

## Managed-copy lifecycle (Cursor agents, Claude skills)

Cursor's runtime loads agent files into memory at launch and Claude's skill loader does not follow symlinks. For both, symlinks are unreliable. The solution: write the contents as a real file (or a real folder, for skills), and record it in a per-root manifest that ties each copy back to its shared source.

Metadata lives in one `.agentic-hub-managed.json` per target root (e.g. `~/.cursor/agents/`, `~/.claude/skills/`), keyed by the copy's path relative to the root:

```json
{ "version": 1, "entries": { "dev/tdd": { "itemId": "skill:dev/tdd", "sourcePath": "/abs/skills/dev/tdd", "sourceHash": "<sha256>" } } }
```

`sourceHash` is the file's sha256 for agents, and the `SKILL.md` sha256 for skill folders. File name and shape are identical to the VS Code extension for migration parity.

Lifecycle:

```
Create:
  target = adapter.target_path_for(item)          # file (agent) or folder (skill)
  copy item.source_path -> target                  # recursive for skill folders
  manifest.entries[rel(target)] = { itemId, sourcePath, sourceHash }

State inspection:
  if target exists AND entry exists AND entry.sourcePath == item.source_path:
      if content_hash(target) == content_hash(source) == entry.sourceHash:
          state = Enabled
      else:
          state = Stale (drifted from shared source)
  if target exists AND no manifest entry:
      state = ForeignFile

Refresh (Replace): re-copy + rewrite the manifest entry (file copies are atomic)
Remove: delete the file/folder + drop its manifest entry (delete manifest when empty)
```

Stale detection vs ForeignFile: managed-copy state is **Stale** only when a manifest entry attributes the target to the same shared source but content has diverged. Without an entry, the file/folder is treated as user-owned (`ForeignFile`) and apply refuses to overwrite.

Manager recovery preserves the same boundary. An unowned stale projection only
stages `desired = true`; the normal plan and explicit Apply produce
`ReplaceManagedCopy`. A suite-owned stale projection resolves the tool's live
binding and re-applies `selected ∪ base ∪ manual extras` through
`api::apply_suite` with `force = false`. Both paths offer validated source-open
and target-reveal fallbacks. Broken and foreign states do not inherit either
automatic remedy.

## Marker-delimited managed-block contract

For `markdown_section_sync` tools (Codex, Claude, OpenClaw, OpenStandard), the rule sync module owns exactly one block in the instruction file:

```md
<!-- agentic-hub:start -->
## Agentic Hub Managed Rules

This section is managed by Agentic Hub. Edit rule selections in the Capability Manager instead of editing these blocks by hand.

### general/precise.mdc

Source: `~/.agentic/rules/general/precise.mdc`
Mirrored link: `~/.codex/agentic-rules/general/precise.mdc`

...rule body with YAML frontmatter stripped...
<!-- agentic-hub:end -->
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

## JSON-section managed-entry contract

For the `json_section` mode (hooks, Codex / Claude / Cursor), `hook_sync` owns a set of marked entries inside each tool's native hook config file rather than a marker block. Instead of textual start/end markers, each managed entry carries an inline `_agenticHub` object:

```json
{
  "command": "/abs/path/script.sh",
  "matcher": "Edit|Write",
  "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "<sha256>", "version": 1 }
}
```

Rules:

- On each sync, read the whole file, partition entries into managed (`_agenticHub` present) vs foreign, and rebuild only the managed set. Foreign entries and all non-`hooks` top-level keys are written back verbatim.
- Cursor uses a flat shape (camelCase event arrays); Codex / Claude use a two-level shape where the marker sits on a dedicated matcher group — a user matcher group is never merged into.
- `${HOOK_DIR}` in `command` is expanded to the hook's source folder at projection time.
- Atomic write (temp + rename). The file is deleted only when the managed set is empty AND the file is hooks-only AND has no foreign keys.
- A malformed target JSON yields `broken` for every hook in that file and blocks the write.

The full schema, event mapping, and CRUD algorithm live in [docs/tech/modules/hook-projection-sync.md](docs/tech/modules/hook-projection-sync.md).

## Components and responsibilities

| Component | Owns |
|-----------|------|
| `scanner` | Shared-root walk (skills/agents/rules/hooks/commands, `__archived__` skipped), validation per kind, `CapabilityItem` construction |
| `adapter_registry` | Per-tool target path resolution, layout strategy, projection kind, hooks file resolution |
| `planner::inspect` | Current per-tool state per item |
| `planner::build_plan` | Diff desired vs current; emit operations; projection-target collision pass; Not-Targeted hook sanitizer |
| `applier` | Execute operations safely; aggregate `ApplyResult`; emit progress events |
| `rule_sync` | Managed-block contract for `markdown_section_sync` tools |
| `hook_sync` | Managed-entry JSON contract + event mapping for `json_section` tools |
| `managed_copy` (sub-module of applier) | Cursor-style copy with metadata sidecar |

## Key flows

### Full apply from desired-state map

```
UI -> cmd_apply
agentic-core::apply_for_tool(tool_id, desired):
    1. scanner::scan_all(settings.sources) -> items (first-source-wins; shadows reported)
    2. adapter = adapter_registry::resolve(settings, tool_id)
    3. states = planner::inspect(items, adapter)
    4. desired = filter_desired_enabled_for_tool(tool_id, desired, items, manifests)  // drops Not-Targeted hooks
    5. ops = planner::build_plan(items, states, desired)
    6. result = applier::apply(ops)   // sync_json_section/clear_json_section batched per hooks file
    7. if adapter.rule_projection == MarkdownSectionSync:
           rule_sync::sync_markdown_rules(adapter, items, post_apply_states)
    8. return result
```

### Refresh-only (no apply)

```
UI -> cmd_inspect
agentic-core::inspect_all_tools():
    1. items = scanner::scan_all(settings.sources)
    2. for each enabled tool:
           adapter = adapter_registry::resolve(settings, tool_id)
           states = planner::inspect(items, adapter)
    3. return items + states_by_tool
```

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| `scanner` | A source folder missing / not a dir | That source contributes nothing + scan error | Other sources still scanned; UI shows error |
| `scanner` | All sources missing | Empty `items` + scan errors | UI shows empty state |
| `scanner` | Same `kind:relative_path` in two sources | Lower-priority item shadowed | First source wins; shadow reported as scan error |
| `scanner` | Permission denied on a source subdir | Subdir items missing | Surface in `errors`; continue scan |
| `scanner` | Symlink cycle inside a source | Walk bounded by max depth | Stop at cycle; log warning |
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
- Managed-copy metadata uses a per-root `.agentic-hub-managed.json` manifest, matching the VS Code extension — do not rename
- Marker-delimited block markers (`agentic-hub:start/end`) match the rebranded VS Code extension verbatim
- Path canonicalization happens once per command; downstream code operates on canonical paths
- Symlink creation is platform-specific; Windows code path is documented in [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) but not exercised in v1

## Related detailed docs

- [docs/tech/modules/rule-projection-sync.md](docs/tech/modules/rule-projection-sync.md) — managed-block format details
- [docs/tech/modules/claude-flat-skill-layout.md](docs/tech/modules/claude-flat-skill-layout.md) — flat layout deep dive
- [docs/tech/modules/openclaw-tool-adapter.md](docs/tech/modules/openclaw-tool-adapter.md) — OpenClaw-specific notes
- [docs/tech/reference/tool-adapter-matrix.md](docs/tech/reference/tool-adapter-matrix.md) — full projection matrix
- [docs/tech/reference/shared-root-contract.md](docs/tech/reference/shared-root-contract.md) — shared root layout contract

## Open questions

- **Managed-copy metadata layout.** Resolved: a single per-root `.agentic-hub-managed.json` manifest (keyed by relative target path), matching the VS Code extension exactly. Chosen over per-file sidecars for migration parity and a clean target directory.
- **Per-rule mirrored files.** The `Mirrored link:` annotation references a real mirrored file at the tool's `rulesPath` if it exists. We do not currently create the mirrored file by default for `markdown_section_sync` tools; the annotation is only added when an external workflow has created one. Decision: keep behavior; document that users who want a real file can configure `rulesPath` and the apply will create the mirror.
- **Concurrent apply across windows.** If the main window and Suite Manager window both trigger apply at the same time, we have a race. Decision: serialize through a Tokio mutex in the Tauri shell layer (one apply at a time, queued).
