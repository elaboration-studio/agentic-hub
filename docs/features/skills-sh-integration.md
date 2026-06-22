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
   skills.sh, open a result on GitHub / skills.sh, and star/unstar favorites. A
   local filter box narrows the starred list as you type (matching name, repo,
   owner, or slug).
3. **Workspace → Install skills** — a floating action button opens a dedicated
   **install window** with a skill × tool matrix; tick the tools for one or more
   starred skills and install them into the active project via the source CLI,
   watching the CLI output stream live with a **Cancel** control. The same local
   filter box narrows the matrix rows; filtering only changes what is shown —
   already-selected skills stay queued even when hidden, and a column "select all"
   applies to the visible rows.
4. **Workspace → Update a skills.sh skill** — when auditing a project, every
   inventory row that the skills.sh CLI installed (matched from the project's
   `skills-lock.json`) shows a `skills.sh` badge. Its row menu gains **Update via
   skills.sh**, which reopens the install window in a focused **update mode** for
   that single skill and runs `npx skills update <name>` with the same live
   streaming, Cancel, and re-scan.

## How the marking works

The skills.sh CLI records project-scoped installs in `<workspace>/skills-lock.json`,
mapping each install **name** (the skill's folder name) to its `source`
(`owner/repo`). The read-only scan reads this lock and matches each skill item by
its leaf folder name, attaching the source so the UI can mark the row and target
an update. Matching is tolerant: a missing or malformed lock marks nothing and
never fails the scan. The lock — not any hub-kept state — is the project-local
source of truth.

## Why local "stars"

skills.sh has **no favorites endpoint**, so starring is a purely local concept.
Favorites are a personal reference list — id, name, source repo, and links —
persisted at `~/.agentic-hub/skills-favorites.json` (overridable). The list is
reused across projects to install the same skills again; the hub keeps **no**
sync state with skills.sh.

## How install and update fit the read-only inventory

Workspace scope is a read-only audit (see
[workspace-inventory](./workspace-inventory.md)). Installing and updating a skill
are the **only two explicit, user-initiated writes** into a workspace — never
automatic, never part of scanning. After the CLI runs, the hub re-scans the
project and the inventory matrix reflects whatever landed. The hub does not
maintain its own record of what is installed; the source CLI's own lock file
(`skills-lock.json`) is the project-local truth and also drives the row badges.

## Decisions

- **Search runs in Rust against the keyless public index.** The `skills.sh/api/v1/*`
  endpoints require a key issued only by emailing Vercel — unusable for ordinary
  users — so the hub uses `https://skills.sh/api/search`, the same keyless endpoint
  the official `skills` CLI uses. It runs in the Rust core (a blocking `reqwest`
  GET behind `cmd_search_skills`), not a WebView `fetch`, because that endpoint
  sends no CORS header. No API key anywhere.
- **Installs run in Rust** via a controlled subprocess (`npx skills add …`), with
  a validated `owner/repo` ref and the workspace dir as cwd. No
  `tauri-plugin-shell`; the WebView stays untrusted.
- **Install has its own window.** Rather than a blocking dialog, the FAB opens a
  separate `install` window (like the command palette) that streams the CLI's
  stdout/stderr line-by-line over a Tauri `Channel` and can **Cancel** the running
  process. Installs run sequentially; one failure never aborts the rest. See
  [skill-sources](../tech/modules/skill-sources.md#the-install-window).
- **Update reuses the install window.** The same window opens in an update mode
  (carried by an `update` field on its mount context) showing one skill and a
  single **Update** button. It runs `npx skills update <name> --project --yes` and
  shares the exact streaming / Cancel / re-scan machinery with install — no
  duplicate surface.

## User flow

1. Config → enable **Skills.sh source**, optionally click **Check CLI** (verifies
   `npx`/Node for installs). No key needed for search.
2. Open the **Skills** tab, type to search (debounced), and **star** the skills
   you want. The starred list below has its own filter box for quickly finding a
   skill in a long list.
3. In the Manager, pick a project in the left rail, click the floating **+**
   action, optionally filter the matrix, tick the target tools for one or more
   starred skills, **Install**.
4. The install window streams each CLI run live; skills install in turn (one
   failure never aborts the rest) and **Cancel** stops the in-flight run. When it
   finishes, the main window re-scans and shows the new skills under each tool
   that has them.
5. To update later, open a marked (`skills.sh`-badged) row's menu and pick
   **Update via skills.sh**. The install window reopens in update mode for that
   one skill; **Update** runs the CLI, **Cancel** stops it, and the re-scan
   refreshes the inventory.

## Limitations / follow-ups

- Target-tool selection is advisory: the `skills` CLI auto-detects agents from
  the project, so the installed set reflects the CLI's detection plus the
  project's existing tool dirs. The re-scan is the source of truth.
- Installs add the whole source repo. Installing only the exact starred skill via
  `--skill <slug>` (the index returns `skillId`) is a follow-up.
- Additional providers (other public registries) via the same `SkillProvider` seam.
