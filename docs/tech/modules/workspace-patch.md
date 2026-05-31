# Module: Workspace Patch

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.workspace.md](../../../ARCHITECTURE.workspace.md), [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/modules/suite-presets.md](./suite-presets.md)
Related Docs: [docs/features/workspace-suite-sync.md](../../features/workspace-suite-sync.md)

## Purpose

Specify the contract for applying a suite into a per-project workspace directory using hard-overwrite copy semantics, with a manifest that supports a clean cycle on next apply.

## Scope model

- `SyncScope = Global | Workspace`. The main window has a header toggle.
- `Global` keeps the existing per-tool home-directory projection (symlinks + managed copies).
- `Workspace` writes into a user-picked project directory using hard copy / managed markdown section.
- Workspace scope supports `Codex`, `Claude`, `Cursor` only (`WORKSPACE_TOOL_IDS`).

## Workspace target store

Persisted at `~/.agentic-hub/state.json` (`workspaceTargets` key) or via `tauri-plugin-store` (decision deferred to M0; the JSON shape is the same):

```json
{
  "workspaceTargets": [
    { "id": "ws-001", "label": "foo", "dir": "/Users/arno/Code/foo", "lastUsedAt": "2026-05-20T01:00:00Z" }
  ],
  "workspaceActiveId": "ws-001"
}
```

Rust API:

```rust
impl WorkspaceTargetStore {
    pub fn read(&self) -> Result<WorkspaceTargetsState>;
    pub fn add(&self, dir: PathBuf) -> Result<WorkspaceTarget>;
    pub fn remove(&self, id: &str) -> Result<()>;
    pub fn set_active(&self, id: &str) -> Result<()>;
    pub fn get_active(&self) -> Result<Option<WorkspaceTarget>>;
}
```

`add()` upserts by canonical path (two entries for the same physical dir are merged), bumps `lastUsedAt`, sets the entry as active, and enforces a 12-entry LRU cap.

## Per-tool target paths

`adapter_registry::create_workspace_adapter(tool_id, workspace_dir)` materializes workspace-scoped `ResolvedAdapter`s:

| Tool | `skills_path` | `agents_path` | `rules_path` | `instructions_path` | `hooks_file` | `rule_projection` |
|------|---------------|---------------|--------------|----------------------|--------------|-------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.agents/agents` | `<ws>/.codex/agentic-rules` | `<ws>/AGENTS.md` | `<ws>/.codex/hooks.json` | `MarkdownSectionSync` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` | `<ws>/.claude/agentic-rules` | `<ws>/CLAUDE.md` | `<ws>/.claude/settings.json` | `MarkdownSectionSync` |
| Cursor | `<ws>/.cursor/skills` | `<ws>/.cursor/agents` | `<ws>/.cursor/rules` | unused | `<ws>/.cursor/hooks.json` | `FileSync` (raw copy) |

Hooks in workspace scope use `json_section` (same as global) against the per-workspace `hooks_file`. OpenClaw is unsupported in workspace scope, so it has no hook target here.

`projection_kind_for(kind, tool)` in workspace mode:

- `Skill` or `Agent` → `managed_copy` (hard copy, dereferencing symlinks)
- `Rule && tool == Cursor` → `managed_copy` (raw copy)
- `Rule && tool == Codex | Claude` → `markdown_section_sync`
- `Hook` → `json_section` (managed JSON entries in the per-workspace `hooks_file`)

Layout: workspace adapters use `Nested` for all tools (Claude's flat-layout constraint applies only to its global home).

## Manifest

Path: `<ws>/.agentic-hub/workspace-patch.json`

Format (version 1):

```json
{
  "version": 1,
  "appliedAt": "2026-05-20T01:00:00.000Z",
  "tool": "codex",
  "suite": { "id": "abc123", "name": "coding-mode" },
  "paths": [
    ".agents/skills/skill-a",
    ".agents/skills/dev/repo-research",
    ".agents/agents/agent-b.md",
    "AGENTS.md::managed-section",
    ".codex/hooks.json::managed-hooks"
  ]
}
```

Conventions:
- Each entry in `paths` is workspace-relative
- The sentinel `<file>::managed-section` tells the cleanup pass to clear the managed `<!-- e-studio-agentic-rules:start -->`/`...:end -->` block in the named file via `rule_sync::sync_markdown_rules` with an empty enabled-rule list, instead of deleting the whole file
- The sentinel `<file>::managed-hooks` tells the cleanup pass to clear this tool's managed hook entries (those carrying the `_agenticHub` marker) in the named JSON file via `hook_sync::sync_json_hooks` with an empty enabled-hook list, preserving foreign entries — instead of deleting the whole file
- Manifest is single-tool. Re-applying with a different focused tool cleans the prior tool's payload using `prior_manifest.tool` for adapter resolution
- Atomic writes: `<manifest>.json.tmp` then `rename`
- The folder `<ws>/.agentic-hub/` is created on first write if missing

## Apply algorithm

`workspace_patch::service::apply_suite({ workspace_dir, tool_id, suite_id })`:

1. Build workspace-scoped adapter via `adapter_registry::create_workspace_adapter`. Reject if the focused tool is OpenClaw or disabled.
2. Read suite via `suite_store::get(suite_id)`. Reject if missing.
3. Scan the source forest for items via `scanner::scan_all(settings.sources)` (first-source-wins).
4. Resolve suite capability ids against scanned items. Items found are queued; ids not provided by any configured source are recorded as `skipped_stale_ids`.
5. Hand off to `workspace_patch::apply::apply`:
   1. `fs::metadata(workspace_dir)`; refuse if not a directory
   2. Resolve `realpath(workspace_dir)` and use it as the boundary for "no writes outside workspace"
   3. Read prior manifest if any. For each prior `paths[i]`:
      - Sentinel `<file>::managed-section`: call `sync_markdown_rules` with empty items for the prior tool's adapter
      - Sentinel `<file>::managed-hooks`: call `hook_sync::sync_json_hooks` with an empty enabled-hook list for the prior tool's adapter (clears managed hook entries, preserves foreign ones)
      - Path: `fs::remove_dir_all_or_file` with `force` semantics; then prune empty parent dirs up to workspace root
   4. For each queued item, hard-copy according to kind:
      - Skill: recursive directory copy, dereferencing symlinks inside
      - Agent / Cursor rule: single file copy
   5. For Codex / Claude rules: build synthetic `ToolCapabilityState[]` with `state=Enabled` and call `sync_markdown_rules` with workspace-scoped adapter
   6. For hooks: build synthetic enabled states for the suite's hook items and call `hook_sync::sync_json_hooks` with the workspace-scoped adapter (skipped for tools whose `hooks_enabled` is off or that the hook does not target)
   7. Build the new manifest: rule writes record `<rel(instructions_path)>::managed-section`; hook writes record `<rel(hooks_file)>::managed-hooks`, so the next cycle can clean them
   8. Write manifest atomically
6. Return `WorkspacePatchResult { tool, workspace_dir, suite_id, suite_name, applied[], removed[], skipped_stale_ids[], notes[], errors[] }`

## Safety rules

- All target paths are validated with the canonical-path guard: `path::resolve(target).starts_with(real_workspace_dir)`. Out-of-workspace targets are recorded as errors and skipped — they do not abort the apply.
- The apply refuses if `<ws>` does not exist or is not a directory
- Markdown rule sync only rewrites the managed marker block — unmanaged content in `AGENTS.md` / `CLAUDE.md` is preserved (existing `rule_sync` behavior)
- Manifest writes are atomic to avoid inconsistent state on crash
- The cleanup pass prunes empty parent directories only up to the workspace root, never beyond it
- Symlinks inside copied skill directories are dereferenced and copied as their resolved file/dir contents — no symlinks cross the workspace boundary

## Canonical-path guard implementation

```rust
fn guard_in_workspace(target: &Path, real_ws: &Path) -> Result<()> {
    let real_target = if target.exists() {
        target.canonicalize()?
    } else {
        let parent = target.parent().ok_or(Error::PathInvalid)?;
        let real_parent = parent.canonicalize()?;
        real_parent.join(target.file_name().unwrap())
    };
    if real_target == *real_ws {
        return Err(Error::OutOfWorkspace(target.to_owned()));
    }
    if !real_target.starts_with(real_ws) {
        return Err(Error::OutOfWorkspace(target.to_owned()));
    }
    Ok(())
}
```

Called before every write inside the apply pipeline.

## Empty-parent pruning

```rust
fn prune_empty_parents(start: &Path, up_to: &Path) -> Result<()> {
    let mut current = start.parent();
    while let Some(p) = current {
        if p == up_to { break; }
        let mut entries = fs::read_dir(p)?;
        if entries.next().is_some() { break; }
        fs::remove_dir(p)?;
        current = p.parent();
    }
    Ok(())
}
```

Pruning stops at the workspace root and never proceeds beyond.

## IPC commands

See [tauri-ipc-contract.md](./tauri-ipc-contract.md) for full schemas. Summary:

- `cmd_pick_workspace_dir() -> WorkspaceTarget`  (opens Tauri folder dialog)
- `cmd_list_workspace_targets() -> WorkspaceTargetsState`
- `cmd_remove_workspace_target(id) -> ()`
- `cmd_set_active_workspace_target(id) -> ()`
- `cmd_apply_workspace_patch({ workspace_id, tool_id, suite_id }) -> WorkspacePatchResult`

## Out of scope

- Per-item enable/disable in workspace mode (suite-only)
- Drift detection between manifest and workspace files
- Watching workspace files for external mutation
- Multiple manifests per workspace (one tool per workspace at a time)
- Config-driven custom workspace target paths (hard-coded per tool in v1)
- OpenClaw workspace support

## Failure modes

| Failure | Impact | Recovery |
|---------|--------|----------|
| Workspace dir disappears between pick and apply | Refused at apply | Surface error |
| Workspace dir is a file | Refused | Same |
| Prior manifest malformed JSON | Treated as empty prior | Log warning; proceed; user can delete manually |
| Cleanup target already removed | Tolerated | Continue |
| New target resolves outside workspace | `OutOfWorkspace` error for that path | Skip; continue |
| Crash mid-apply | Atomic manifest preserves prior state | Next apply does a best-effort cleanup; user can re-apply |
| Disk full mid-copy | Partial copy; some files written, manifest not updated | Next apply may have orphan files; cleanup tolerates them; user can clear manifest manually if needed |

## Testing strategy

### Unit

- `WorkspaceTargetStore` add/remove/set_active with LRU cap
- Canonical-path guard against various path-traversal inputs
- Sentinel parsing in manifest cleanup
- Manifest atomic write + read roundtrip

### Integration

- Apply suite into tempdir workspace; verify per-tool files exist
- Re-apply with different suite; verify prior payload cleaned, new payload present
- Re-apply with different tool; verify prior tool's managed block cleared
- Out-of-workspace target via symlink trickery → recorded as error, no write
- Symlink inside source skill dir → resolved content copied, no symlink in workspace
- Empty parent dir pruning stops at workspace root

## Open questions

- Should the manifest store source content hashes for drift detection? Defer; v1 is hard-overwrite anyway
- Should we offer a "show manifest contents" affordance in the UI? Defer; manifest is at a known path, user can `cat` it
- Should we add a "Clear Workspace Patch" button that runs cleanup without re-applying? Defer; users can apply an empty suite for the same effect
