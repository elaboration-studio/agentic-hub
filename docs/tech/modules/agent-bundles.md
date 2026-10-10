# Agent bundles

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-10-10
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md), [suite-presets.md](./suite-presets.md), [hook-projection-sync.md](./hook-projection-sync.md)
Related Docs: [ehub-cli-contract.md](../reference/ehub-cli-contract.md), [docs/features/agent-bundles.md](../../features/agent-bundles.md)

## Purpose

Turn a suite into an **agent**: a named, versioned set of capabilities that another program can mount for one run of one CLI, without changing the CLI's global setup or the user's project. Global suite apply keeps working exactly as before; bundles are a second, read-only-to-the-world output of the same scan.

## Why bundles, not projection

Projection writes into a tool home and stays there. Two agents that share one CLI on one Mac (a CTO run and a CPO run in Cursor at the same time) would fight over the same `~/.cursor/skills`. A bundle is a separate directory per agent version and harness. The caller hands it to the CLI with a per-process flag (`--plugin-dir`) or folds it into the prompt. Nothing shared is written, so concurrent runs cannot collide.

## Model changes (`agentic-core`)

### `SuiteDefinition.agent`

```rust
pub struct AgentSpec {
    pub emoji: Option<String>,          // ≤ 16 chars
    pub instructions: Option<String>,   // markdown, ≤ 32 KiB
    pub required_clis: Vec<String>,     // ids from resources/cli-tools/catalog.json, ≤ 20
}
// SuiteDefinition gains:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub agent: Option<AgentSpec>,
```

The suites file keeps `version: 1`. Older hubs ignore the field when reading; a suite saved by an older hub simply loses it (documented, acceptable for a single-user dotfile).

### `CapabilityKind::Mcp`

- Directory-marker kind like hooks: `<root>/mcp/<id>/mcp.json`, id prefix `mcp`.
- `mcp.json` (`agentic-hub.mcp.v1`):

```json
{ "$schema": "agentic-hub.mcp.v1", "name": "github", "transport": "stdio",
  "command": "github-mcp", "args": ["stdio"], "env": ["GITHUB_TOKEN"] }
```

  `transport` is `stdio` (needs `command`) or `http` (needs `url`). `env` lists variable **names** only; a value-bearing object is rejected at parse time.
- **No global projection in v1.** The adapter registry returns no target for `mcp`, so the planner never plans an op for it, and the UI shows MCP items as "bundle only". Suites can include them; bundles carry them.

## Version hash (`agent_version.rs`)

`agent_version(suite, scan) -> String` — SHA-256 over a canonical byte stream, first 12 hex chars:

1. `BUNDLE_LAYOUT_VERSION` (u32 constant; bump when the on-disk layout changes).
2. Suite `id`, `name`, `description`, and the `agent` block (serde_json with sorted keys).
3. Capability ids sorted. For each: its source bytes. Files hash their bytes. Directories hash each file's relative path and bytes in sorted path order; symlinks inside a skill dir hash their target string, never followed.
4. Capabilities that the scan cannot find are hashed as `missing:<id>` so the version still changes when one appears.

Timestamps and the base suite are excluded.

## Bundle builder (`bundle.rs`)

`build_bundle(ctx, suite_id, harness) -> Result<BundleManifest, BundleError>`

### Layout

Root: `~/.agentic-hub/bundles/<suiteId>/<version>/<harness>/`.

| Harness | Mount | Contents |
| --- | --- | --- |
| `claude` | plugin | `plugin/.claude-plugin/plugin.json`, `plugin/skills/<leaf>/`, `plugin/agents/<leaf>.md`, `plugin/commands/<leaf>.md`, `plugin/hooks/hooks.json` |
| `cursor` | plugin | `plugin/.cursor-plugin/plugin.json`, `plugin/skills/<leaf>/`, `plugin/agents/<leaf>.md`, `plugin/commands/<leaf>.md`, `plugin/hooks/hooks.json` |
| `codex` | prompt | `skills/<leaf>/` |
| `grok` | prompt | `skills/<leaf>/` |

Every harness also gets `instructions.md` (when non-empty) and `bundle.json` (the manifest, written last).

- **Instructions** are the agent's `instructions`, then each rule body in capability-id order, separated by a blank line and a `<!-- rule:<id> -->` comment. Rules are folded into instructions for every harness so a rule means the same thing everywhere.
- **Copies, not links.** Every file is copied, so a bundle is a snapshot that later edits to the source cannot change.
- **Leaf names** collide when two capabilities share a basename (`dev/tdd`, `cto/tdd`). The first by id wins; the rest go to `skipped` with reason `name collision`.
- **Hooks** reuse `hook_sync`'s manifest parsing and per-tool event translation. `${HOOK_DIR}` becomes the copied hook dir's absolute path. In prompt mode hooks go to `skipped`.
- **Agents and commands** in prompt mode go to `skipped`.
- **MCP** goes into `manifest.mcpServers` for every harness; the caller decides what it can mount.
- **Required CLIs** reuse the `cli_tools` preflight probe; the result is reported, never enforced here.
- `plugin.json` name is `ehub-<slugified suite name>`, version is the agent version.

### Atomic, lock-free, idempotent

1. If `<root>/bundle.json` exists, read and return it (touch the version dir's mtime).
2. Else build into a sibling temp dir `<root>.tmp-<pid>-<nanos>`.
3. `rename(temp, root)`. If the rename fails because `root` now exists, another process won the race: delete the temp dir and return the winner's manifest.

Because a finished bundle only ever appears through one rename, readers never see a half-written one, and two processes building the same version at once both succeed. No file lock is needed.

### Garbage collection

After a successful build, delete sibling version dirs under `bundles/<suiteId>/` whose mtime is older than 7 days. The current version is never deleted. A run holds a bundle for at most its wall-clock limit (eCanvas: 60 minutes), well inside the window.

## CLI (`cli.rs` in core, dispatch in the bin)

- `agentic_core::cli::run(args: &[String], out: &mut impl Write) -> i32` parses argv by hand (four commands, no new dependency), runs, writes one JSON document, returns the exit code from the contract.
- `crates/agentic-hub/src/main.rs` checks argv before `run()`: basename `ehub`, or first arg `ehub`, means headless. Tauri is never initialised on that path.
- On app setup, `cli_link::ensure(current_exe)` keeps `~/.agentic-hub/bin/ehub` as a symlink to the app binary: create when missing, retarget when it is a symlink elsewhere, leave a real file alone and log it (same never-overwrite rule as projection).

## Concurrency

| Case | Outcome |
| --- | --- |
| Two runs of different agents on one CLI | Different bundle dirs; no shared write |
| Two runs of the same agent version | Same bundle, read-only |
| Suite edited during a run | New version builds a new dir; the running bundle is untouched |
| Two processes build the same version | One rename wins; the other reuses it |
| GUI applies a suite while the CLI builds | Bundles never touch tool homes; projection never touches bundles |

## Security

- The CLI is local, read-only outside `bundles/`, and spawns nothing except the existing `cli_tools` probe.
- Suite id and harness are validated before use; the bundle root is built from validated parts only, never from caller paths.
- MCP manifests carry env var names, never values.
- Hooks in a bundle run the same commands the user already authored for global projection; no new trust is granted.

## Tests (TDD)

- `agent_version`: stable across runs; changes on rule edit, skill file edit, capability add/remove, agent block edit; ignores timestamps; symlink inside skill not followed.
- `mcp` kind: scan finds `mcp/<id>/mcp.json`; parse rejects env values, missing command/url; planner plans nothing for it.
- `bundle`: layout per harness; rules folded into instructions; collisions skipped; hooks translated with absolute dir; prompt mode skips hooks/agents/commands; reuse on second build; concurrent build race (two threads) both succeed with one dir; GC deletes only stale non-current versions.
- `cli`: each command's JSON shape; exit codes 2/3/4; bad id rejected before IO.
- `cli_link`: creates, retargets symlink, leaves real file.
