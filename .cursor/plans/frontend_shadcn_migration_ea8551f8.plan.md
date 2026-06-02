---
name: Frontend shadcn migration
overview: Migrate the Agentic Hub UI layer to a components-first stack — shadcn/ui on Tailwind v4 with Zustand state — and formalize the existing dark developer-tool aesthetic into a DESIGN.md design spec, replacing the hand-rolled components and the ~1200-line styles.css.
todos:
  - id: tooling
    content: Add deps (tailwind v4, @tailwindcss/vite, zustand, shadcn runtime deps, @types/node); configure @/* alias in tsconfig.json + vite.config.ts; add @tailwindcss/vite plugin; create index.css Tailwind entry; run shadcn init to produce components.json + lib/utils.ts
    status: completed
  - id: design-md
    content: "Run product-design-system: write DESIGN.md formalizing the dark aesthetic (token table, typography, component stylings, depth, do's/don'ts, responsive, agent prompt guide); generate + open the preview HTML"
    status: completed
  - id: tokens
    content: Encode DESIGN.md tokens into index.css :root/.dark (shadcn vars + custom --success/--warning/--kind-agent), set radius and base color
    status: completed
  - id: ui-components
    content: Generate the needed shadcn/ui components into src/components/ui/ (button, input, label, select, checkbox, dialog, alert-dialog, dropdown-menu, badge, card, table, tabs, toggle, toggle-group, alert, progress, sonner, separator, tooltip)
    status: completed
  - id: stores
    content: Build Zustand stores in src/state/ (manager, suites, workspace), extracting the existing scan/inspect/stage/apply, suite CRUD, and workspace logic
    status: completed
  - id: migrate-shell
    content: Migrate App.tsx (layout-only) + Header + ActionBar to shadcn + manager store; wire Tabs/ToggleGroup nav and Sonner toaster
    status: completed
  - id: migrate-manager
    content: Migrate Matrix, ToolCells, EmptyState, ConflictDialog to shadcn (Table, Button/Toggle cells, Badge, DropdownMenu row actions, Dialog) preserving tri-state + tree/flat logic
    status: completed
  - id: migrate-config
    content: Migrate ConfigPage panels to Card/Select/Input/Switch + suites/workspace stores
    status: completed
  - id: migrate-suites
    content: Migrate SuitesPage to shadcn (Card two-pane, Checkbox with indeterminate replacing TriCheckbox, Select, AlertDialog delete)
    status: completed
  - id: migrate-workspace
    content: Migrate WorkspacePanel to shadcn (Card, RadioGroup/Select, Button, Alert results)
    status: completed
  - id: cleanup-verify
    content: Delete styles.css + dead hand-rolled components, update main.tsx import; update ARCHITECTURE.md stack decisions + repo layout; run pnpm build + lint, boot tauri dev, verify route parity
    status: completed
isProject: false
---

# Frontend shadcn migration

Migrate the `src/` UI from hand-rolled components + a single `styles.css` to a **components-first** stack: **shadcn/ui** on **Tailwind v4**, with **Zustand** stores replacing prop-drilled `useState`. In parallel, run `/product-design-system` to produce `DESIGN.md` that formalizes the current dark aesthetic into the shadcn token set.

The Rust core, Tauri shell, IPC contract ([src/ipc.ts](/Users/ArnoYe/Developer/agentic-hub/src/ipc.ts)), and generated [src/types/](/Users/ArnoYe/Developer/agentic-hub/src/types/index.ts) are **untouched** — this is a pure view-layer migration.

## Scope (Tier-1 confirmations folded into this plan)

Approving this plan authorizes:
- New dependencies: `tailwindcss@4`, `@tailwindcss/vite`, `zustand`, plus shadcn's runtime deps (`class-variance-authority`, `clsx`, `tailwind-merge`, `tw-animate-css`, `lucide-react`, the `@radix-ui/*` primitives shadcn pulls per component) and `@types/node`.
- A `src/` restructure into `components/` + `state/` + `lib/`.
- Deleting [src/styles.css](/Users/ArnoYe/Developer/agentic-hub/src/styles.css) and the hand-rolled component bodies once their shadcn replacements land.
- A stack-decision + repo-layout edit to [ARCHITECTURE.md](/Users/ArnoYe/Developer/agentic-hub/ARCHITECTURE.md).

## Target structure

```
src/
  main.tsx                 # imports ./index.css
  App.tsx                  # thin: layout + hash routing only
  index.css                # Tailwind v4 entry + design tokens (replaces styles.css)
  lib/utils.ts             # cn() helper
  components/
    ui/                    # shadcn generated primitives
    layout/Header.tsx, ActionBar.tsx
    manager/Matrix.tsx, ToolCells.tsx, EmptyState.tsx, ConflictDialog.tsx
    config/ConfigPage.tsx
    suites/SuitesPage.tsx
    workspace/WorkspacePanel.tsx
  state/manager.ts, suites.ts, workspace.ts   # Zustand
  ipc.ts                   # unchanged
  shared.tsx               # trimmed: constants/helpers only (Banner/ToolCells move out)
  types/                   # unchanged
```

## Component mapping (hand-rolled -> shadcn)

- `.btn` / `.btn-ghost` / `.btn-danger` -> `Button` (`default` / `ghost` / `destructive` / `secondary`)
- `.banner` -> `Alert`
- `ConflictDialog` modal -> `Dialog`; `window.confirm` deletes -> `AlertDialog`
- `<select.suite-tool>` -> `Select`; `<input.path-input/matrix-search>` -> `Input` (+ `Label`)
- `TriCheckbox` / tool checkboxes -> `Checkbox` (native indeterminate support)
- nav tabs (Manager/Suites/Config) -> `Tabs`; scope + flat/tree toggles -> `ToggleGroup`; watch pill -> `Toggle`
- `.badge` / `.kind-badge` / `.cap-chip` -> `Badge` (variants + custom semantic tokens)
- `.matrix table` -> `Table`; `.config-panel` / `.workspace` / `.suitebar` -> `Card`
- row-actions `<details>` menu -> `DropdownMenu`
- apply progress -> `Progress`; result summaries -> `Sonner` toasts
- toggle cells (custom on/mod/warn) -> small `Button`/`Toggle` styled with Tailwind tokens (keep the existing tri-state logic)

shadcn components to generate: `button input label select checkbox dialog alert-dialog dropdown-menu badge card table tabs toggle toggle-group alert progress sonner separator tooltip`.

## Zustand stores (extract existing logic verbatim)

- `state/manager.ts` — the scan/inspect/stage/apply loop currently inside [src/App.tsx](/Users/ArnoYe/Developer/agentic-hub/src/App.tsx) (`refresh`, `desired` map, `toggle`/`toggleMany`, `runApply`, watcher/progress listeners). `App.tsx` becomes layout-only.
- `state/suites.ts` — suite list + draft + CRUD from [src/SuitesPage.tsx](/Users/ArnoYe/Developer/agentic-hub/src/SuitesPage.tsx).
- `state/workspace.ts` — targets/active/apply from [src/WorkspacePanel.tsx](/Users/ArnoYe/Developer/agentic-hub/src/WorkspacePanel.tsx).

## Design tokens (DESIGN.md -> index.css `.dark`)

Map today's palette in [styles.css](/Users/ArnoYe/Developer/agentic-hub/src/styles.css) to the shadcn variable set, keeping the look:
- `--background` `#0e0f13`; `--card`/`--popover` `#16181f`; `--secondary`/`--muted` `#1d2029`
- `--border`/`--input` `#272a33`; `--foreground` `#e6e8ee`; `--muted-foreground` `#9aa0ad`
- `--primary` `#4f46e5` (indigo); `--destructive` `#ef4444`; `--ring` indigo-based; `--radius` `0.5rem`
- custom semantics (no shadcn default): `--success` `#22c55e`, `--warning` `#f59e0b`, `--kind-agent` `#a855f7`
- Typography: keep the native system UI stack as Body (deliberate, justified for a desktop tool) + `ui-monospace`/JetBrains Mono for code/paths. Dark-first; light mode tokens defined but secondary.

DESIGN.md is written to the project root next to PRODUCT.md, with the full token table, typography hierarchy, component stylings, depth/elevation, do's/don'ts, responsive, and an Agent Prompt Guide. The product-design-system preview HTML is generated and opened during execution.

## Verification

After migration: `pnpm build` (runs `tsc --noEmit && vite build`) must pass, `pnpm tauri dev` boots the window, and `cargo`/Rust side is untouched. Walk each route (Manager flat+tree, Suites CRUD, Workspace apply, Config) for visual + behavioral parity.

## Notes / risks

- Tailwind v4 uses `@tailwindcss/vite` + a single `@import "tailwindcss";` in `index.css` (no `tailwind.config.js`); shadcn's latest CLI scaffolds this. `tw-animate-css` replaces `tailwindcss-animate`.
- `@/*` alias must be added in **both** [tsconfig.json](/Users/ArnoYe/Developer/agentic-hub/tsconfig.json) (`baseUrl`+`paths`) and [vite.config.ts](/Users/ArnoYe/Developer/agentic-hub/vite.config.ts) (`resolve.alias`).
- Strict TS (`noUnusedLocals/Parameters`) — keep generated components clean and lint green.
- Behavioral parity is the bar: keep the tri-state toggle, tree/flat logic, conflict take-over, and stage-then-apply semantics exactly as-is; only the presentation changes.