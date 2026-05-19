# Architecture: Workspace Patch

This document deepens the workspace-scope projection design for Agentic Hub. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for the full system view.

## Context from root architecture

The root architecture establishes two scopes: **Global** (writes into tool homes under the user's home directory using symlinks and managed copies) and **Workspace** (writes into a user-picked project directory using hard copies and managed markdown blocks). This doc owns the Workspace scope: its hard-copy semantics, its manifest cycle, its safety guards, and how it integrates with suite apply.

## Why this domain is split out

Workspace scope uses a fundamentally different filesystem contract than global scope:

- **Hard copy, not symlinks.** Many work repos sync over Dropbox / iCloud / Windows where symlinks are flaky. Workspace projection dereferences symlinks and copies file/dir contents.
- **Manifest-driven cleanup.** Global scope inspects current disk state and produces a plan. Workspace scope reads the prior apply's manifest and cleans exactly what the manager previously wrote, then writes the new payload. There is no per-item state inspection.
- **Single-tool, single-suite.** Workspace scope applies one suite to one tool at a time. The manifest is single-tool.
- **Out-of-workspace guard.** Every target path is validated against the canonicalized workspace root before any write.

Folding all of that into the root architecture or into projection.md would conflate two distinct lifecycle models. Splitting it lets each be read independently.

## Goals

- Define the workspace target store and LRU semantics
- Define per-tool workspace target paths (different from global)
- Define the manifest format and atomic write semantics
- Define the clean-then-write apply cycle
- Define the out-of-workspace path guard
- Define how rule sync integrates with the manifest cleanup

## Non-goals

- Per-item enable/disable in workspace scope (suite is the only entry point)
- OpenClaw workspace projection (out of scope per the original VS Code spec)
- Drift detection between manifest and workspace files
- Watching workspace files for external mutation
- Multiple manifests per workspace (one tool per workspace at a time)
- Config-driven custom workspace target paths (hard-coded per tool for v1)

## Scope and boundaries

In scope:
- `workspace_patch` module in `agentic-core`
- `workspace_target_store` (LRU of recent workspace dirs)
- Workspace-scoped adapter overrides for Codex / Claude / Cursor
- Manifest read / write / cleanup cycle
- Integration with `suite_store` for the apply entry point

Out of scope:
- Global-scope projection (see [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md))
- Tauri capability layer (see [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md))
- UI presentation (see [docs/features/workspace-suite-sync.md](docs/features/workspace-suite-sync.md))

## Existing system and reuse

This module is a 1:1 port of the VS Code extension's:

- `WorkspaceTargetStore` (TS) → `workspace_target_store` (Rust)
- `WorkspacePatchService` (TS) → `workspace_patch::service` (Rust)
- `WorkspacePatchApplyService` (TS) → `workspace_patch::apply` (Rust)

The behavior is preserved. The folder under the workspace is renamed from `.e-studio-copilot/` to `.agentic-hub/`.

## Trust model

```
+--------------------+            +---------------------------+
|  WebView           |  IPC only  |  agentic-core             |
|  picks workspace   | ---------> |  - validates dir exists   |
|  via dialog        |            |  - canonicalizes path     |
|                    |            |  - stores in LRU          |
+--------------------+            |  - returns canonical path |
                                   +---------------------------+
                                                |
                                                v
                                   +---------------------------+
                                   |  workspace_patch::apply   |
                                   |  - guards every write     |
                                   |    against canonical root |
                                   +---------------------------+
```

The capability layer cannot statically scope FS access to user-picked workspace dirs (they are unknown at build time). The Rust core enforces the guard. See [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) for the broader trust model.

## Workspace target store

Persisted in `~/.agentic-hub/state.json` (via `tauri-plugin-store` or direct JSON, decision deferred to M0):

```json
{
  "workspaceTargets": [
    { "id": "ws-001", "label": "foo", "dir": "/Users/arno/Code/foo", "lastUsedAt": "2026-05-20T..." },
    { "id": "ws-002", "label": "bar", "dir": "/Users/arno/Code/bar", "lastUsedAt": "2026-05-19T..." }
  ],
  "workspaceActiveId": "ws-001"
}
```

API:

```rust
impl WorkspaceTargetStore {
    pub fn read(&self) -> WorkspaceTargetsState { /* ... */ }
    pub fn add(&mut self, dir: PathBuf) -> WorkspaceTarget {
        // canonicalize; upsert by canonical path; bump lastUsedAt; cap at 12 entries (LRU eviction)
        // also set as active
    }
    pub fn remove(&mut self, id: &str) { /* ... */ }
    pub fn set_active(&mut self, id: &str) { /* ... */ }
    pub fn get_active(&self) -> Option<WorkspaceTarget> { /* ... */ }
}
```

Cap: 12 entries. LRU eviction on `add`. The `label` defaults to the directory basename and is editable in the UI.

## Per-tool workspace target paths

`ToolAdapterRegistry::create_workspace_adapters(tools, workspace_dir)` materializes workspace-scoped `ResolvedAdapter`s:

| Tool | `skills_path` | `agents_path` | `rules_path` | `instructions_path` | `rule_projection` |
|------|---------------|---------------|--------------|----------------------|-------------------|
| Codex | `<ws>/.agents/skills` | `<ws>/.agents/agents` | `<ws>/.codex/agentic-rules` | `<ws>/AGENTS.md` | `markdown_section_sync` |
| Claude | `<ws>/.claude/skills` | `<ws>/.claude/agents` | `<ws>/.claude/agentic-rules` | `<ws>/CLAUDE.md` | `markdown_section_sync` |
| Cursor | `<ws>/.cursor/skills` | `<ws>/.cursor/agents` | `<ws>/.cursor/rules` | unused | `file_sync` (raw copy) |

Projection kind in workspace mode:

- `kind == Skill || kind == Agent` → `managed_copy` (hard copy, dereference symlinks)
- `kind == Rule && tool == Cursor` → `managed_copy` (raw file copy into `<ws>/.cursor/rules`)
- `kind == Rule && tool == Codex || Claude` → `markdown_section_sync` (managed block in `AGENTS.md` / `CLAUDE.md`)

Codex projects under `<ws>/.agents/` rather than `<ws>/.codex/` because OpenAI Codex's documented skill scan paths are `$CWD/.agents/skills` walking up to `$REPO_ROOT/.agents/skills`. Agents follow the same `.agents/` root for consistency.

Layout strategy: workspace adapters use `Nested` for all tools (the flat-layout constraint only applies to Claude's global `~/.claude/skills/` loader, not to workspace paths).

## Manifest format

Path: `<ws>/.agentic-hub/workspace-patch.json`

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
    ".agents/agents/group/agent-c.md",
    "AGENTS.md::managed-section"
  ]
}
```

Conventions:

- Each entry in `paths` is workspace-relative
- The sentinel `<file>::managed-section` tells the cleanup pass to clear the managed marker block in the named file via `rule_sync::sync_markdown_rules` with an empty enabled-rule list, instead of deleting the whole file
- Manifest is single-tool; re-apply with a different focused tool cleans the prior tool's payload using the prior manifest's `tool` field
- Atomic write: write to `<ws>/.agentic-hub/workspace-patch.json.tmp`, then rename
- Folder `<ws>/.agentic-hub/` is created on first write if missing

## Apply algorithm

`workspace_patch::apply({ workspace_dir, tool_id, suite_id })`:

```
1. Read suite from suite_store; reject if missing.
2. Scan shared root for items (via scanner).
3. Resolve suite capability ids against scanned items:
     queued_items = items where item.id in suite.capabilities
     skipped_stale_ids = capability ids in suite but not in scan
4. Build workspace-scoped adapter via adapter_registry::create_workspace_adapter(tool_id, workspace_dir).
   Reject if tool is OpenClaw or disabled.
5. fs::metadata(workspace_dir); refuse if not a directory.
6. real_ws = workspace_dir.canonicalize()
7. Read prior manifest if any at <ws>/.agentic-hub/workspace-patch.json.
8. For each entry in prior_manifest.paths:
     if entry ends with "::managed-section":
         file = real_ws.join(entry.before("::"))
         rule_sync::sync_markdown_rules(prior_adapter_for_prior_tool, items=[], states=[])
            -> empties the managed block, preserves unmanaged content
            -> may delete the file if managed block was the entire content
     else:
         target = real_ws.join(entry)
         guard_in_workspace(target, real_ws)?
         fs::remove_dir_all_or_file(target)
         prune_empty_parent_dirs(target, up_to=real_ws)
9. For each queued_item:
     target = adapter.target_path_for(item)
     guard_in_workspace(target, real_ws)?
     match item.kind:
       Skill:  copy_dir_recursive_dereferencing(item.source_path, target)
       Agent:  copy_file(item.source_path, target)
       Rule (Cursor only):  copy_file(item.source_path, target)
10. For Codex/Claude rules:
      enabled_rules = queued_items where kind==Rule
      states = synthetic [(item, Enabled) for item in enabled_rules]
      rule_sync::sync_markdown_rules(adapter, items=enabled_rules, states=states)
      record sentinel in manifest.paths: "<relative(instructions_path)>::managed-section"
11. Build new manifest with paths = collected writes.
12. fs::write atomic to <ws>/.agentic-hub/workspace-patch.json
13. Return WorkspacePatchResult { tool, workspace_dir, suite_id, suite_name,
                                    applied[], removed[], skipped_stale_ids[],
                                    notes[], errors[] }
```

## Safety rules

### Out-of-workspace path guard

Every target path is validated:

```rust
fn guard_in_workspace(target: &Path, real_ws: &Path) -> Result<()> {
    let real_target = if target.exists() {
        target.canonicalize()?
    } else {
        // path doesn't exist yet; canonicalize the parent and rejoin
        let parent = target.parent().ok_or(Error::PathInvalid)?;
        let real_parent = parent.canonicalize()?;
        real_parent.join(target.file_name().unwrap())
    };
    if real_target == *real_ws { return Err(Error::OutOfWorkspace(target.to_owned())); }
    if !real_target.starts_with(real_ws) { return Err(Error::OutOfWorkspace(target.to_owned())); }
    Ok(())
}
```

If a target resolves outside `real_ws`, the path is **skipped** and recorded as an error in the result. The apply continues with remaining items.

### Dereference symlinks on copy

When copying a skill directory or rule file, any symlinks encountered are dereferenced and copied as their resolved file/dir contents. No symlinks ever cross the workspace boundary. This is critical because:

- The workspace is often Dropbox / iCloud / git-tracked, where symlinks break
- Following a symlink that points outside `~/.agentic/` would write external file content into the workspace, which is correct behavior (the user opted into that content by enabling the capability)

### Atomic manifest write

```rust
fn write_manifest_atomic(path: &Path, manifest: &WorkspacePatchManifest) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(manifest)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
```

If a crash happens mid-apply between the FS writes and the manifest write, the manifest still reflects the prior state. The next apply reads the prior manifest, cleans the prior payload (some of which is already gone — `fs::remove_*` with `force` semantics tolerates that), and writes the new payload. The cleanup pass is idempotent.

### Prune empty parent dirs

After removing a file via the cleanup pass, walk up the parents and `rmdir` empty ones, stopping at the workspace root. This keeps the workspace tidy. The workspace root itself is never removed.

```rust
fn prune_empty_parents(start: &Path, up_to: &Path) -> Result<()> {
    let mut current = start.parent();
    while let Some(p) = current {
        if p == up_to { break; }
        if fs::read_dir(p)?.next().is_some() { break; }
        fs::remove_dir(p)?;
        current = p.parent();
    }
    Ok(())
}
```

### Markdown rule sync preserves unmanaged content

For Codex/Claude rules, the workspace-mode rule sync calls the same `rule_sync::sync_markdown_rules` as global mode, with workspace-scoped adapter settings (`instructions_path = <ws>/AGENTS.md` etc.). The marker contract is identical; unmanaged content in `AGENTS.md` / `CLAUDE.md` is preserved verbatim.

## Re-apply cycle

```
Round 1:
  user: workspace=/ws, suite=coding-mode, tool=codex, click Apply
  result: <ws>/.agents/{skills,agents}/* + <ws>/AGENTS.md managed block + manifest

Round 2 (same workspace, different suite):
  user: workspace=/ws, suite=writing-mode, tool=codex, click Apply
  cleanup reads prior manifest -> removes coding-mode payload
  writes writing-mode payload -> new manifest

Round 3 (same workspace, different tool):
  user: workspace=/ws, suite=writing-mode, tool=claude, click Apply
  cleanup reads prior manifest -> removes ALL writing-mode-for-codex payload
                                  (including AGENTS.md managed block via sentinel)
  writes writing-mode-for-claude payload -> new manifest with tool=claude
```

The manifest's `tool` field is the cleanup-time selector for which adapter to use when interpreting `::managed-section` sentinels.

## Components and responsibilities

| Component | Responsibility |
|-----------|----------------|
| `workspace_target_store` | LRU persistence of recent workspace dirs; canonicalize on add |
| `adapter_registry::create_workspace_adapter` | Build workspace-scoped `ResolvedAdapter` for one tool |
| `workspace_patch::service::apply_suite` | Top-level orchestrator: scan, resolve, validate, delegate |
| `workspace_patch::apply::apply` | The actual clean-then-write algorithm; calls into `applier` and `rule_sync` for primitives |
| `applier::copy_dir_recursive_dereferencing` | Skill directory copy with symlink dereferencing |
| `applier::copy_file` | Single file copy |
| `rule_sync::sync_markdown_rules` | Managed block writer (shared with global) |

## Key flows

### Apply workspace patch

```
UI -> cmd_apply_workspace_patch({ workspace_id, tool_id, suite_id })
  agentic-hub bin -> workspace_patch::service::apply_suite(...)
    -> resolved as detailed in Apply algorithm above
    -> emit progress events
    -> return WorkspacePatchResult
UI -> render summary
```

### Pick workspace dir

```
UI -> cmd_pick_workspace_dir
  agentic-hub bin -> dialog::open_folder_dialog (Tauri plugin)
  user picks dir
  -> validate exists + is_dir
  -> canonicalize
  -> workspace_target_store::add(dir)
  -> return WorkspaceTarget { id, label, dir, lastUsedAt }
UI -> update store; refresh workspace tab
```

## Failure modes

| Component | Failure | Impact | Recovery |
|-----------|---------|--------|----------|
| `cmd_pick_workspace_dir` | User cancels dialog | No-op | UI unchanged |
| `cmd_pick_workspace_dir` | Picked dir does not exist (race) | Refused | Surface error; LRU not updated |
| `workspace_target_store` | `state.json` write fails | LRU lost across restarts | Surface; in-memory state preserved |
| `apply` | Workspace dir does not exist at apply time | Refused | No FS writes; surface error |
| `apply` | Workspace dir is a file | Refused | Same |
| `apply` | Prior manifest malformed JSON | Treat as empty prior | Log warning; proceed |
| `apply` | Cleanup target already gone | Tolerated | Continue with remaining cleanup |
| `apply` | Cleanup target is a real file matching a tracked path | Removed (tracked = ours) | Manifest is the source of truth for ownership |
| `apply` | New target lives outside workspace (e.g. via canonicalization through symlink) | `OutOfWorkspace` error | Skip that path; continue |
| `apply` | Disk full mid-copy | Single op fails | Partial apply; manifest tracks what was written |
| `apply` | Crash between FS writes and manifest write | Manifest stale | Next apply cleans stale state (some files may have been written but not tracked; user can re-apply or manually clean) |
| `rule_sync` (workspace) | Markers malformed in `<ws>/AGENTS.md` | Reported; no rewrite | User fixes manually |
| `applier::copy_dir_recursive_dereferencing` | Symlink cycle inside source skill dir | Bounded by depth | Stop at cycle; log warning |

## Operational rules

- Manifest is the only source of truth for workspace ownership. The cleanup pass is driven exclusively by the manifest, not by FS heuristics.
- Workspace target paths are hard-coded per tool in v1. Users who want different paths must wait for a config-driven design.
- Never write to the workspace root itself; always at least one path component deeper.
- Always prune empty parent dirs up to the workspace root, never beyond.
- Always canonicalize the workspace dir at the start of every apply (not just at pick time) — the user may have moved the dir between picks and applies.

## Related detailed docs

- [docs/features/workspace-suite-sync.md](docs/features/workspace-suite-sync.md) — user-facing feature spec
- [docs/tech/modules/workspace-patch.md](docs/tech/modules/workspace-patch.md) — implementation details, atomic write, sentinel format
- [docs/tech/modules/suite-presets.md](docs/tech/modules/suite-presets.md) — suite definition the workspace patch consumes
- [docs/tech/modules/rule-projection-sync.md](docs/tech/modules/rule-projection-sync.md) — managed block contract (shared)

## Open questions

- **Gitignore guidance.** The `.agentic-hub/` folder should typically be gitignored. Should the app offer to write a `.gitignore` entry on first apply? Decision: no automatic gitignore mutation in v1; document the recommendation in README and as a tooltip in the UI.
- **Multiple workspaces with overlapping payloads.** If a user applies different suites to two project workspaces that happen to overlap (e.g. nested git repos), the cleanup of one workspace could touch files under the other. The canonical-path guard prevents writes outside the apply's workspace root, but a write under `<ws-A>/<ws-B>/...` could shadow `<ws-B>`'s own files. Decision: documented as out-of-scope edge case; users should keep workspaces non-overlapping.
- **Workspace-scope OpenClaw.** Currently refused at the adapter layer. Revisit if OpenClaw adds a stable project-level skills path.
- **Per-item enable/disable in workspace scope.** Currently only suite-driven. A future "edit workspace patch" affordance would require a different data model (or just inline the manifest as a synthetic suite). Defer.
