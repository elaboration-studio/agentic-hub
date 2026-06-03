# Module: Rule Projection Sync

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/features/mvp-unified-agentic-capability-manager.md](../../features/mvp-unified-agentic-capability-manager.md), [docs/tech/modules/openclaw-tool-adapter.md](./openclaw-tool-adapter.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md)

## Purpose

Define how shared `rules/` content is projected into Cursor, Codex, Claude Code, and OpenClaw. Different tools demand different filesystem models for rules, so the projection layer supports three modes and picks one per tool.

## Projection modes

| Mode | Mechanism | Source-of-truth file |
|------|-----------|----------------------|
| `link_sync` | Create or repair symlinks under a tool-owned rules directory | Shared rule file on disk |
| `file_sync` | Create a managed copied file plus metadata that ties the target back to the shared source | Shared rule file on disk; target is a managed copy |
| `markdown_section_sync` | Maintain one marker-delimited section inside a top-level instruction file | Shared rule file on disk; tool reads from the instruction file |

## Tool defaults (global scope)

| Tool | Rule projection | Primary target | Notes |
|------|-----------------|----------------|-------|
| Cursor | `link_sync` | `~/.cursor/rules` | Preserves nested folder structure |
| Codex | `markdown_section_sync` | `~/.codex/AGENTS.md` | Optional mirrored rule files at `~/.codex/agentic-rules/` are annotated when present |
| Claude Code | `markdown_section_sync` | `~/.claude/CLAUDE.md` | `~/.claude/rules` stays configurable but unused by default |
| OpenClaw | `markdown_section_sync` | `~/.openclaw/workspace/SOUL.md` | SOUL.md is loaded at every session start; see [openclaw-tool-adapter.md](./openclaw-tool-adapter.md) |

## Tool defaults (workspace scope)

| Tool | Rule projection | Primary target |
|------|-----------------|----------------|
| Cursor | `file_sync` (raw copy) | `<ws>/.cursor/rules/` |
| Codex | `markdown_section_sync` | `<ws>/AGENTS.md` |
| Claude | `markdown_section_sync` | `<ws>/CLAUDE.md` |

## Ownership and scope

- Shared `~/.agentic/rules/**` files remain the source of truth
- The manager owns exactly one block per `markdown_section_sync` target file
- All content outside the managed block is preserved verbatim
- Workspace-mode rule sync uses workspace-scoped instruction-file paths but the same marker contract

## Managed markdown block contract

The manager owns one block per instruction file:

```md
<!-- agentic-hub:start -->
## Agentic Hub Managed Rules

This section is managed by Agentic Hub. Edit rule selections in the Capability Manager instead of editing these blocks by hand.

### general/precise.mdc

Source: `~/.agentic/rules/general/precise.mdc`
Mirrored link: `~/.codex/agentic-rules/general/precise.mdc`

...rule body with YAML frontmatter stripped...

### general/workspace.mdc

Source: `~/.agentic/rules/general/workspace.mdc`

...rule body with YAML frontmatter stripped...
<!-- agentic-hub:end -->
```

Rules:

1. **One block per file.** The manager refuses to write multiple blocks. If a second start marker is found before the end marker, the state is `MalformedMarkers` and no rewrite happens.
2. **Inline rule content** with YAML frontmatter (everything between two leading `---` lines) stripped before insertion.
3. **`Source:` line** is required for every rule entry; uses tilde-prefixed home-relative path.
4. **`Mirrored link:` line** is optional and added only when a real mirrored file exists at `<tool.rulesPath>/<rule.relativePath>` (e.g. for Codex when `~/.codex/agentic-rules/general/precise.mdc` exists).
5. **Preserve all content outside the markers** on every rewrite.
6. **Empty enabled-rules list** → remove only the managed block, preserving the rest of the file. Delete the file only if the managed block was the entire file.
7. **No marker pair, no enabled rules** → no-op.
8. **No marker pair, enabled rules present** → append the managed block after existing content, separated by a blank line.

### Markers preserved verbatim

The opening and closing markers — `<!-- agentic-hub:start -->` and `<!-- agentic-hub:end -->` — are identical to the rebranded VS Code extension's markers. This lets users migrate from the extension to Agentic Hub without touching their instruction files.

## Conflict handling

| Condition | Outcome |
|-----------|---------|
| Instruction path is a directory | `ErrConflictDirectoryAtTarget`; no write |
| Instruction path is another non-file target | `ErrConflictNonFileTarget`; no write |
| Markers malformed (start without end, or end before start) | `RuleSyncError::MalformedMarkers`; no rewrite until user fixes manually |
| Multiple start markers | `RuleSyncError::MalformedMarkers` |
| Permission denied on read or write | `ErrPermissionDenied`; preserve in-memory state |

For `markdown_section_sync` tools, state inspection is derived from the instruction file contents, not from the tool's `rulesPath`. A rule's per-tool state is `enabled` if it appears in the managed block with a matching `Source:` line, otherwise `disabled` (or `broken` if markers are malformed).

## YAML frontmatter stripping

Rules with frontmatter:

```mdc
---
description: Be precise and clear
---
> Be precise.
```

Inlined as:

```md
> Be precise.
```

Algorithm:

1. If the first non-whitespace line is exactly `---`, find the next `---` line.
2. Strip everything from the start through the closing `---` (inclusive) and the following newline.
3. Trim leading whitespace.
4. Use the result as the inlined body.

If no opening `---` is found, the entire file content is the body.

## Mirrored rule files

For tools where the optional `rulesPath` is configured (e.g. Codex's `~/.codex/agentic-rules/` or OpenClaw's `~/.openclaw/agentic-rules/`):

- The manager creates symlinks under `<tool.rulesPath>` for every enabled rule, preserving nested folder structure
- The `Mirrored link:` annotation is added to the managed-block entry only when this real file exists
- Removing a rule from enabled set also removes the mirrored symlink

Mirrored files give the user a real on-disk file they can grep / `cat` independently of the managed block. They are not required for the rule to take effect — the managed block is what each tool actually reads.

## Rust API

```rust
pub struct RuleSyncInput<'a> {
    pub adapter: &'a ResolvedAdapter,
    pub items: &'a [CapabilityItem],
    pub states: &'a [ToolCapabilityState],
}

pub enum RuleSyncOutcome {
    Wrote,
    Removed,
    NoOp,
}

pub enum RuleSyncError {
    DirectoryAtTarget(PathBuf),
    NonFileAtTarget(PathBuf),
    MalformedMarkers(PathBuf),
    PermissionDenied(PathBuf),
    Io(std::io::Error),
}

pub fn sync_markdown_rules(input: RuleSyncInput) -> Result<RuleSyncOutcome, RuleSyncError>;
```

## Algorithm

```
fn sync_markdown_rules(input):
    target = input.adapter.instructions_path
    enabled_rules = filter items where kind=Rule and state=Enabled
    if target does not exist:
        if enabled_rules empty: return NoOp
        mkdir -p target.parent
        write managed_block(enabled_rules) to target
        return Wrote
    content = read_to_string(target)
    case content markers:
        BOTH start and end, start before end:
            new_content = replace block contents with managed_block(enabled_rules)
            if enabled_rules empty:
                new_content = remove block entirely + trim trailing blank lines
                if new_content.trim().is_empty():
                    fs::remove_file(target)
                    return Removed
            atomic_write(target, new_content)
            return Wrote
        NEITHER marker:
            if enabled_rules empty: return NoOp
            new_content = content + "\n\n" + managed_block(enabled_rules)
            atomic_write(target, new_content)
            return Wrote
        ONE marker only OR end before start OR multiple starts:
            return Err(MalformedMarkers)
```

## Workspace-mode integration

None. Rule projection is a **global-scope** write operation. Workspace scope is a
read-only inventory ([workspace-inventory.md](./workspace-inventory.md)): it reads
a project's `AGENTS.md` / `CLAUDE.md` as inventory rows but never rewrites the
managed block in a workspace.

## Tests

- Unit:
  - Frontmatter stripping handles missing frontmatter, empty frontmatter, multiline frontmatter
  - Marker detection: correct, missing, malformed, double-start
  - Managed-block diff against fixtures
- Integration (tempfile):
  - Create new instruction file with managed block
  - Refresh existing managed block, preserve unmanaged content above and below
  - Empty enabled set removes block; full file deletion only if managed block was entire file
  - Malformed markers reported; no rewrite
- Cross-tool:
  - Codex sync writes to `<ws>/AGENTS.md` in workspace mode
  - Same `sync_markdown_rules` produces equivalent output in global and workspace mode given equivalent adapter inputs

## Open questions

- Should we add a per-rule `last_synced` timestamp inside the managed block? Defer; not needed for current behavior
- Should `Mirrored link:` be required (always create the mirrored file)? Decision: optional; the managed block is enough for tools that read it
