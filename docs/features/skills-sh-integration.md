# Feature: Skills.sh as a Pluggable Workspace Skill Source

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-06-04
Related Docs: [docs/tech/modules/skill-sources.md](../tech/modules/skill-sources.md), [docs/features/workspace-inventory.md](./workspace-inventory.md), [docs/tech/modules/tauri-ipc-contract.md](../tech/modules/tauri-ipc-contract.md)

## What it is

An opt-in integration with [skills.sh](https://skills.sh) — a public registry of
agent skills — that lets a user **search**, **star**, and **install** skills into
a workspace project. It is the first concrete *skill source* behind a provider
seam so other public registries can plug in later.

Three surfaces:

1. **Config → Skills.sh source** — an enable toggle, an optional override for
   where starred skills are stored, and a "Check CLI" button. No API key: search
   uses the keyless public index.
2. **Skills page** (a header tab shown only when the source is enabled) — search
   skills.sh, open a result on GitHub / skills.sh, and star/unstar favorites.
3. **Workspace → Install skills** — a floating action button opens a skill × tool
   matrix; tick the tools for one or more starred skills and install them into the
   active project via the source CLI in a single batch.

## Why local "stars"

skills.sh has **no favorites endpoint**, so starring is a purely local concept.
Favorites are a personal reference list — id, name, source repo, and links —
persisted at `~/.agentic-hub/skills-favorites.json` (overridable). The list is
reused across projects to install the same skills again; the hub keeps **no**
sync state with skills.sh.

## How install fits the read-only inventory

Workspace scope is a read-only audit (see
[workspace-inventory](./workspace-inventory.md)). Installing a skill is the **one
explicit, user-initiated write** into a workspace — never automatic, never part
of scanning. After the install CLI runs, the hub re-scans the project and the
inventory matrix reflects whatever landed. The hub does not maintain its own
record of what is installed; the source CLI's own lock file
(`skills-lock.json`) is the project-local truth.

## Decisions

- **Search runs in Rust against the keyless public index.** The `skills.sh/api/v1/*`
  endpoints require a key issued only by emailing Vercel — unusable for ordinary
  users — so the hub uses `https://skills.sh/api/search`, the same keyless endpoint
  the official `skills` CLI uses. It runs in the Rust core (a blocking `reqwest`
  GET behind `cmd_search_skills`), not a WebView `fetch`, because that endpoint
  sends no CORS header. No API key anywhere.
- **Installs run in Rust** via a controlled subprocess (`npx skills add …`), with
  a validated `owner/repo` ref and the workspace dir as cwd. No
  `tauri-plugin-shell`; the WebView stays untrusted and calls a typed
  `cmd_install_skill`.

## User flow

1. Config → enable **Skills.sh source**, optionally click **Check CLI** (verifies
   `npx`/Node for installs). No key needed for search.
2. Open the **Skills** tab, type to search (debounced), and **star** the skills
   you want.
3. Switch the Manager to **Workspace** scope, pick a project, click the floating
   **+** action, tick the target tools for one or more starred skills in the
   matrix, **Install**.
4. Each skill installs in turn (one failure never aborts the rest); the matrix
   re-scans and shows the new skills under each tool that has them.

## Limitations / follow-ups

- Target-tool selection is advisory: the `skills` CLI auto-detects agents from
  the project, so the installed set reflects the CLI's detection plus the
  project's existing tool dirs. The re-scan is the source of truth.
- Installs add the whole source repo. Installing only the exact starred skill via
  `--skill <slug>` (the index returns `skillId`) is a follow-up.
- Additional providers (other public registries) via the same `SkillProvider` seam.
