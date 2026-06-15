# Release notes

Notes for the **current release only** — the release workflow publishes this
file verbatim as the GitHub Release body. For the full version history see
[CHANGELOG.md](CHANGELOG.md); for how releases are built and published see
[DEPLOYMENT.md](DEPLOYMENT.md).

## [0.8.3] — 2026-06-15

### Highlights

- **Agents stored in subfolders now load in Cursor and Codex.** If you organize
  agent specs in folders (e.g. `zoom/cto.md`), the hub used to mirror that
  folder into the tool's agents directory — but Cursor and Codex only read
  agents at the top level, so those agents silently never appeared. The hub now
  flattens Cursor and Codex agents to a single file (`cto.md` / `cto.toml`)
  where the tool can find them. Claude is unchanged (it reads nested agents). If
  two agents from different folders share a filename, one wins deterministically
  and the other is reported as a conflict, and any leftover nested copy from the
  old layout is cleaned up automatically.
- **Enabling agents for Codex now actually works.** Previously, turning on an
  agent for Codex looked successful but Codex never picked it up — for two
  reasons. Older configs pointed Codex agents at the shared `~/.agents` root
  instead of `~/.codex/agents`, and even at the right path the hub linked the
  raw markdown spec, which Codex ignores (it loads only `.toml` subagents). Both
  are fixed: the path is migrated automatically on launch, and Codex agents are
  now written as proper `<name>.toml` subagent files (`name`, `description`,
  `developer_instructions`) derived from your markdown spec. Any leftover
  markdown link the hub had created is cleaned up; files you authored yourself
  are never touched.

### Migration

- **Automatic.** The Codex agents path fix is applied once on launch, and the
  old nested agent copies are cleaned up the next time you enable agents. No
  manual steps are needed.
