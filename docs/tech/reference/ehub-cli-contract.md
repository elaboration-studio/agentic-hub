# eHub CLI contract (v1)

Status: Accepted
Mode: Detailed
Owner: Arno
Last Updated: 2026-10-10
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [ARCHITECTURE.projection.md](../../../ARCHITECTURE.projection.md)
Related Docs: [docs/tech/modules/agent-bundles.md](../modules/agent-bundles.md), [docs/features/agent-bundles.md](../../features/agent-bundles.md), eCanvas `docs/decisions/ADR-057-ehub-agent-profiles.md`

The headless `ehub` command is the only door other local programs use to read eHub agents and get a run bundle. eCanvas's Mac runner is the first caller. The contract is versioned: a breaking change bumps `contract` and keeps v1 answering until callers move.

## Where it lives

- **Path:** `~/.agentic-hub/bin/ehub`. The Agentic Hub app keeps it as a symlink to its own executable on every launch. A real file at that path is never replaced.
- **Same binary as the app.** When invoked as `ehub` (argv[0] basename) or as `<app> ehub …`, the binary answers one command and exits. It never starts Tauri, never opens a window, and never touches the network.
- **Works with the app closed.** It reads the same files the app reads: `~/.agentic-hub/config.json`, the suites file, and the source roots.

## Rules

- Output is always one JSON document on stdout. Nothing else is written to stdout. Diagnostics go to stderr.
- Every command accepts and ignores `--json` (output is JSON either way), so callers can pass it for clarity.
- The CLI is read-only on everything except `~/.agentic-hub/bundles/`.
- Ids and harness names are validated before any filesystem access. A suite id matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.

## Commands

### `ehub version`

```json
{ "schema": 1, "contract": 1, "appVersion": "0.17.0" }
```

### `ehub agents list`

Every suite is an agent candidate. `isAgent` is true when the suite has an `agent` block.

```json
{
  "schema": 1,
  "agents": [
    {
      "id": "3f1c…",
      "name": "Arno's CTO",
      "emoji": "⚒️",
      "description": "Architecture, delivery, code review",
      "version": "a1b2c3d4e5f6",
      "isAgent": true,
      "capabilityCounts": { "skill": 12, "agent": 2, "rule": 3, "hook": 1, "command": 0, "mcp": 1 },
      "requiredClis": ["gh"]
    }
  ]
}
```

### `ehub agents show <id>`

```json
{ "schema": 1, "agent": { "…AgentSummary": "…", "capabilities": ["skill:cto/tdd", "rule:cto/core", "mcp:github"] } }
```

### `ehub bundle <id> --harness <cursor|claude|codex|grok>`

Builds (or reuses) the immutable bundle for the suite's current version and that harness, and prints its manifest.

```json
{
  "schema": 1,
  "bundle": {
    "agentId": "3f1c…",
    "name": "Arno's CTO",
    "version": "a1b2c3d4e5f6",
    "harness": "cursor",
    "mount": "plugin",
    "root": "/Users/me/.agentic-hub/bundles/3f1c…/a1b2c3d4e5f6/cursor",
    "pluginDir": "/Users/me/.agentic-hub/bundles/3f1c…/a1b2c3d4e5f6/cursor/plugin",
    "instructionsFile": "/Users/me/.agentic-hub/bundles/3f1c…/a1b2c3d4e5f6/cursor/instructions.md",
    "skills": [{ "name": "tdd", "path": "/Users/me/.agentic-hub/bundles/…/cursor/plugin/skills/tdd" }],
    "mcpServers": [
      { "name": "github", "transport": "stdio", "command": "github-mcp", "args": ["stdio"], "envNames": ["GITHUB_TOKEN"] }
    ],
    "requiredClis": [{ "id": "gh", "installed": true }],
    "skipped": [{ "capability": "hook:audit", "reason": "prompt mount has no hook support" }]
  }
}
```

- `mount` is `plugin` for `cursor` and `claude`, `prompt` for `codex` and `grok`.
- `pluginDir` is null in prompt mode. `instructionsFile` is null when the agent has no instructions and no rules.
- Every path in the manifest is absolute and inside `root`.
- `mcpServers` never carries secret values — only the names of environment variables the server reads.

## Errors

```json
{ "schema": 1, "error": { "code": "NOT_FOUND", "message": "No suite with id 3f1c…" } }
```

| Exit | `code` | When |
| --- | --- | --- |
| 0 | — | Success |
| 1 | `INTERNAL` | IO or parse failure |
| 2 | `INVALID_ARGUMENT` | Unknown command, bad id, missing flag |
| 3 | `NOT_FOUND` | No suite with that id |
| 4 | `UNSUPPORTED_HARNESS` | Harness not in the list above |

## Versioning

- `version` is the first 12 hex characters of a SHA-256 over the suite definition (minus timestamps), the agent block, the sorted capability ids, every capability's source bytes, and the bundle layout version. Same inputs, same version, on any Mac.
- The base suite is not part of an agent bundle. The base is what the user projects globally; an agent is what a run mounts on top.
