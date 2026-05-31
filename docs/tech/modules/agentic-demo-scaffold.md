# Module: Agentic Demo Scaffold

Status: Draft
Mode: Detailed
Owner: Arno
Last Updated: 2026-05-20
Depends On: [ARCHITECTURE.md](../../../ARCHITECTURE.md), [docs/features/agentic-demo-scaffold.md](../../features/agentic-demo-scaffold.md)
Related Docs: `resources/agentic-demo/README.md` (bundled template), [docs/tech/reference/shared-root-contract.md](../reference/shared-root-contract.md)

## Purpose

Ship a one-shot command that materializes a sane default **shared agentic root** (same contract as the capability manager: `skills/`, `agents/`, `rules/`) so first-time users can wire Codex, Claude, Cursor, and OpenClaw without copying a personal repo by hand.

## Behavior

- IPC command: `cmd_scaffold_demo({ mode })`
- Target directory: `settings.shared_root` (default `~/.agentic`), normalized like other paths
- Source: bundled tree under `resources/agentic-demo/`, embedded into the Tauri binary at build time via `include_dir!`
- Modes:
  - **merge** — write only missing paths (safe reruns)
  - **overwrite** — replace files that exist with bundled template copies (do not delete user-added files)

## Bundled content

```
resources/agentic-demo/
  README.md
  ONBOARD.md
  package.json
  scripts/skills-manage.mjs
  .skill-lock.json
  skills-lock.json
  skills/
    agentic-hub-setup/SKILL.md
    npx-skills-workflow/SKILL.md
  agents/
    agent-resources-manager.md
  rules/
    general/workspace.mdc
  hooks/
    auto-format-after-edit/hook.json
    auto-format-after-edit/script.sh
```

- `README.md`, `ONBOARD.md` — orientation and the optional `npx skills` lock workflow (aligned with `~/.agentic-arno` patterns)
- Demo skills teach the manager's contract by example
- Demo agent shows the `<agentsPath>/<file>.md` convention
- Demo rule shows the `rules/<category>/<name>.mdc` convention
- Demo hook (`auto-format-after-edit`) shows the `hooks/<name>/hook.json` + sibling `script.sh` convention and the `${HOOK_DIR}` token, so the hook flow is dogfoodable end-to-end (see [hooks-projection.md](../../features/hooks-projection.md))

## Embedding strategy

```rust
use include_dir::{include_dir, Dir};

static DEMO_TREE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../resources/agentic-demo");

pub fn scaffold_demo(shared_root: &Path, mode: ScaffoldMode) -> Result<ScaffoldResult> {
    let mut written = 0u32;
    let mut skipped = 0u32;
    let mut replaced = 0u32;
    let mut errors = Vec::new();

    fs::metadata(shared_root).ok()
        .filter(|m| m.is_dir())
        .or_else(|| { fs::create_dir_all(shared_root).ok(); fs::metadata(shared_root).ok() })
        .ok_or(Error::PathIsFile(shared_root.to_owned()))?;

    for entry in DEMO_TREE.find("**/*").unwrap() {
        match entry {
            include_dir::DirEntry::File(file) => {
                let rel = file.path();
                let target = shared_root.join(rel);
                match (mode, target.exists()) {
                    (ScaffoldMode::Merge, true) => { skipped += 1; }
                    (ScaffoldMode::Merge, false) => {
                        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
                        atomic_write(&target, file.contents())?;
                        written += 1;
                    }
                    (ScaffoldMode::Overwrite, true) => {
                        atomic_write(&target, file.contents())?;
                        replaced += 1;
                    }
                    (ScaffoldMode::Overwrite, false) => {
                        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
                        atomic_write(&target, file.contents())?;
                        written += 1;
                    }
                }
            }
            include_dir::DirEntry::Dir(_) => { /* dirs created lazily via parent of files */ }
        }
    }

    Ok(ScaffoldResult { shared_root: shared_root.to_owned(), written, skipped, replaced, errors })
}
```

## Atomic write

Each bundled file is written via:

```rust
fn atomic_write(target: &Path, contents: &[u8]) -> Result<()> {
    let tmp = target.with_extension(format!("{}.tmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("")));
    fs::write(&tmp, contents)?;
    fs::rename(tmp, target)?;
    Ok(())
}
```

Atomic write prevents partial-content files on crash.

## Integration

- The capability manager main window listens for "shared root empty" (zero scanned items + dir does not exist or contains no `skills`/`agents`/`rules` children) and renders an empty-state CTA wired to `cmd_scaffold_demo({ mode: 'merge' })`
- The Settings menu exposes a "Scaffold Demo Resources…" option that lets the user pick `merge` vs `overwrite`
- After scaffold, the UI re-runs `cmd_scan` and renders the resulting inventory

## Safety rules

- Refuse if `shared_root` resolves to a regular file (not a directory) — `ErrPathIsFile`
- Refuse if `shared_root` is outside the user's home directory (sanity guard against accidental mis-configuration writing to `/`)
- Path components from the bundled tree are sanitized (no `..`, no absolute paths) — `include_dir` enforces this by construction
- The scaffold writes **only** under `shared_root`; never under tool homes

## Result envelope

```rust
pub enum ScaffoldMode { Merge, Overwrite }

pub struct ScaffoldResult {
    pub shared_root: PathBuf,
    pub written: u32,
    pub skipped: u32,
    pub replaced: u32,
    pub errors: Vec<ScaffoldError>,
}

pub struct ScaffoldError {
    pub path: PathBuf,
    pub message: String,
    pub code: String,
}
```

## IPC

See [tauri-ipc-contract.md](./tauri-ipc-contract.md):

- `cmd_scaffold_demo({ mode: 'merge' | 'overwrite' }) -> ScaffoldResult`

## Tests

- Unit:
  - Empty target: all bundled files written
  - Pre-populated target in `merge` mode: existing files preserved, missing files written
  - Pre-populated target in `overwrite` mode: existing files replaced, user-added files outside the bundle preserved
  - Target is a file: refused
  - Target outside home directory: refused
- Integration:
  - Run scaffold, then `cmd_scan` finds the bundled skills, agents, and rule

## Failure modes

| Failure | Recovery |
|---------|----------|
| Permission denied on shared_root | Surface error; partial scaffold tolerated |
| Disk full | Partial scaffold; errors reported per file |
| Bundled tree missing at build time | Build failure (compile-time `include_dir!`) |

## Open questions

- Should the bundle be updatable post-install via a separate command (e.g. download from a CDN)? Decision: no in v1; bundle is build-time only
- Should we offer additional bundle templates (e.g. "minimal", "full") in v1? Decision: no; one well-curated bundle is enough
- Should scaffold create a `.gitignore` inside `~/.agentic/`? Decision: yes, with sensible defaults for `node_modules` and lock files — added to the bundled tree itself, not generated dynamically
