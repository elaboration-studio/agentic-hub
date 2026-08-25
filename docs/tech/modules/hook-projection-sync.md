# Module: Hook Projection Sync

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-31
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [docs/features/hooks-projection.md](../../features/hooks-projection.md)
Related Docs: [docs/tech/modules/rule-projection-sync.md](./rule-projection-sync.md), [docs/tech/reference/tool-adapter-matrix.md](../reference/tool-adapter-matrix.md), [docs/tech/modules/tauri-ipc-contract.md](./tauri-ipc-contract.md)

## Purpose

Define the on-disk contract and CRUD algorithm for projecting shared hooks into Cursor / Claude Code / Codex hook configuration files, with safe co-existence next to user-authored entries. This is the `json_section` projection mode — the fourth mode alongside `link_sync`, `file_sync`, and `markdown_section_sync`.

The core module is `hook_sync`, a sibling of `rule_sync`. It is a 1:1 port of the VS Code extension's `HookProjectionSyncService` + `HookEventMapper`; the behavior is preserved verbatim, the implementation is Rust.

## Source-of-truth schema

```rust
pub struct HookManifest {
    pub id: String,                  // kebab-case, unique within a source
    pub name: Option<String>,        // display name, defaults to id
    pub description: Option<String>,
    pub events: Vec<HookEventSpec>,  // 1..N
    pub command: String,             // shell command; may contain ${HOOK_DIR}
    pub timeout: Option<u32>,        // seconds
    pub loop_limit: Option<u32>,     // Cursor-only safety knob; positive int
    pub targets: Option<Vec<ToolId>>,// defaults to [cursor, claude, codex]
}

pub struct HookEventSpec {
    pub name: HookCanonicalEvent,    // PascalCase, e.g. PostToolUse
    pub matcher: Option<String>,     // tool name / pattern; some events ignore it
}
```

`sourceHash` is `sha256(canonical_json(manifest))`, where the canonical form is the parsed JSON minus the `$schema` field with stable key order. The `$schema` value `agentic-hub.hook.v1` is preserved verbatim from the extension.

## Event name mapping

Canonical names follow the Claude/Codex PascalCase set. Cursor uses camelCase; `hook_sync::event_map` translates at projection time and owns the per-tool supported-event sets.

| Canonical (source) | Cursor (camelCase) | Codex supported | Notes |
|--------------------|--------------------|-----------------|-------|
| `PreToolUse` | `preToolUse` | yes | matcher = tool-name regex |
| `PostToolUse` | `postToolUse` | yes | matcher = tool-name regex |
| `PostToolUseFailure` | `postToolUseFailure` | no | Codex skips with note |
| `UserPromptSubmit` | `beforeSubmitPrompt` | yes | matcher unused |
| `Stop` | `stop` | yes | matcher unused |
| `SessionStart` | `sessionStart` | yes | matcher = `startup` / `resume` / `clear` |
| `SessionEnd` | `sessionEnd` | no | Codex skips with note |
| `PreCompact` | `preCompact` | yes | matcher = `manual` / `auto` |
| `PostCompact` | (no map) | yes | Cursor skips with note |
| `Notification` | (no map) | no | Both skip with note |
| `PermissionRequest` | (no map) | yes | Cursor skips with note |

Unsupported event/tool combinations are **not errors**. They produce a per-tool note such as `"PostCompact is not supported by Cursor; entry skipped."` surfaced in the apply result.

## Rust API

```rust
pub struct HookSyncInput<'a> {
    pub adapter: &'a ResolvedAdapter,        // resolves the target file + tool id
    pub items: &'a [CapabilityItem],         // all scanned items
    pub manifests: &'a HashMap<String, HookManifest>, // by item id
    pub states: &'a [ToolCapabilityState],   // desired/current enablement
}

pub enum HookSyncOutcome { Wrote, Removed, NoOp }

pub enum HookSyncError {
    ForeignFile(PathBuf),       // target exists but is not a regular file
    Broken(PathBuf),            // target JSON failed to parse
    PermissionDenied(PathBuf),
    Io(std::io::Error),
}

pub fn sync_json_hooks(input: HookSyncInput) -> Result<HookSyncOutcome, HookSyncError>;
```

## CRUD algorithm

```
read target JSON (or {} if missing)
parse:
    fail            -> state = broken; abort write (HookSyncError::Broken)
    ok:
        partition entries: mine (has _agenticHub) vs theirs
        compute desired managed entries from enabled hooks for this tool
        new hooks block = theirs + desired managed entries
        write temp file + rename
```

### Cursor (`hooks.json`) — flat shape

A managed entry sits directly inside the event array:

```json
{
  "version": 1,
  "hooks": {
    "postToolUse": [
      { "command": "/path/script.sh", "matcher": "Edit|Write",
        "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "abc", "version": 1 } },
      { "command": "user-hook.sh" }
    ]
  }
}
```

- `version` defaults to `1` when the file is created.
- `loopLimit` from the manifest is written as `loop_limit` on the managed entry.
- Foreign top-level keys (e.g. `$schema`) are preserved.

### Claude Code (`settings.json`) and Codex (`hooks.json`) — two-level shape

The marker sits on the **matcher group**:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [ { "type": "command", "command": "/path/script.sh", "timeout": 30 } ],
        "_agenticHub": { "hookId": "auto-format-after-edit", "sourceHash": "abc", "version": 1 }
      }
    ]
  }
}
```

- Each managed hook gets its own dedicated matcher group with one inner entry. Agentic Hub never appends into a user matcher group.
- All non-`hooks` top-level keys in Claude's `settings.json` are passed through verbatim.

## Inspection algorithm

For a given `(item, tool)`:

1. Resolve the target file. If it does not exist → `disabled`.
2. `symlink_metadata` the path. If not a regular file → `foreign_file`.
3. Read + parse. On parse error → `broken`.
4. Look for any managed entry whose `_agenticHub.hookId == item.id`:
   - None found → `disabled`.
   - Found, `sourceHash` matches → `enabled`.
   - Found, `sourceHash` differs → `stale`.
5. `target_path` and `current_link_target` are both set to the target file path.

## `${HOOK_DIR}` expansion

`command` strings are expanded at projection time: every `${HOOK_DIR}` is replaced with the absolute path of the hook's source folder (`item.source_path`). Expansion happens once, when building the managed entry — the written file always contains absolute paths.

## Atomic write

```
mkdir -p dir
write tmp = serde_json::to_string_pretty(next) + "\n"
rename tmp -> target
```

An empty managed set does **not** delete the target file when it has foreign entries or other settings. The file is removed only when the resulting `hooks` block is empty AND the file is hooks-only (Cursor/Codex) AND there are no foreign top-level keys.

## Validation

The loader returns a typed error when:

- `id` is missing or not kebab-case
- `events` is empty or contains an unknown event name
- `command` is missing
- `targets` contains an unknown tool id
- `loop_limit` is present and not a positive integer

Items that fail validation surface as `invalid` in the manager, consistent with skills / agents / rules.

## Not-Targeted sanitizer

Before planning, `filter_desired_enabled_for_tool(tool_id, desired, items, manifests)` strips any hook whose effective targets exclude `tool_id` from the desired-enabled set, returning one note per dropped item. Effective targets = `manifest.targets` ∩ `{tools where hooks_enabled}`. This guarantees a stale UI toggle cannot reach the planner and become a silent no-op.

## Settings-managed internal hooks

`agentic-core::internal_hooks` contributes virtual hook items and manifests for
Agentic Hub-owned behavior. The initial registry entry is the local usage tracer
(`hook:agentic-hub-usage-tracer-{tool}`), whose source identity is
`Agentic Hub` / `agentic-hub`. Capture tools are Codex, Claude, Cursor, Kiro,
and Grok. Grok uses `GrokHookFile` (one Claude-style JSON file under
`~/.grok/hooks/`) rather than `json_section`.

These hooks are included in scans and inspection results when enabled by
settings, but they are not suite-selectable and are locked in Manager/palette
toggles. `api::sync_hooks` merges enabled internal hooks into every hook sync for
their target tool, independent of the caller's desired map, so Manager applies,
suite full resets, and watcher reconcile cannot accidentally remove them.
Virtual hooks use a stable manifest hash instead of reading `hook.json` from a
source root.

## Workspace mode

None. Hook projection is a **global-scope** write operation. Workspace scope is a
read-only inventory ([workspace-inventory.md](./workspace-inventory.md)) and does
not write hook files. (Inventorying installed workspace hooks is a noted
follow-up.)

## Concurrency

Per-file writes are serialized inside the service via an in-process lock keyed by absolute target path. At the IPC layer, hook sync runs under the shared `apply_lock` mutex (see [tauri-ipc-contract.md](./tauri-ipc-contract.md)), so no two applies touch the same file concurrently.

## Failure modes

| Symptom | Surfaced as | Recovery |
|---------|-------------|----------|
| Target file is a directory | `foreign_file` | Move/remove the directory; re-apply |
| JSON parse error | `broken` | Fix or delete the file; re-apply |
| Source hook fails validation | `item.valid = false` | Fix `hook.json`; refresh scan |
| Unsupported `(event, tool)` | per-tool note (not an error) | Expected; informational |
| Codex feature flag `hooks=false` | out of scope, user-owned | Document: user keeps `hooks=true` in `config.toml` |

## Tests

- Unit:
  - Manifest parsing incl. `loopLimit`; validation rejections (bad id, empty events, missing command, unknown target)
  - Event mapping: canonical ↔ Cursor camelCase; per-tool supported sets; unsupported combos produce notes
  - `sourceHash` stability across key reordering
  - `${HOOK_DIR}` expansion
- Integration (tempfile):
  - Marker-preserving CRUD on Cursor flat shape and Claude/Codex two-level shape
  - Foreign entries + non-`hooks` keys preserved across sync
  - Stale detection on `sourceHash` mismatch; broken on malformed JSON
  - Empty managed set: file kept when foreign keys present; removed when hooks-only + empty
  - Not-Targeted sanitizer strips ineligible hooks before plan; notes emitted

## Open questions

- Should we expose a per-tool toggle to suppress unsupported-event notes? Default: always surface.
- Should the apply result include a dry-run JSON diff of the change? Defer; the inspector chip + reason field is enough for v1.
