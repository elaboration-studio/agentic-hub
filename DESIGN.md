# Design System — Agentic Hub

Source of truth for the Agentic Hub UI. The app is a single-user Tauri 2.x
desktop tool for projecting shared agentic capabilities across AI coding tools.
The interface is a dense, data-first control surface — it should feel like a
precise developer instrument, not a marketing page.

Implementation: tokens live in [src/index.css](src/index.css) as CSS variables
consumed by Tailwind v4 (`@theme inline`) and shadcn/ui components in
[src/components/ui/](src/components/ui/).

## Visual Theme & Atmosphere

A calm workbench where the data is the hero. Dark surfaces use graphite steps;
light surfaces use clean neutral layers, so structure reads through contrast
rather than borders or shadows. One confident indigo carries every primary
action; semantic color is rationed and only appears to mean something (on /
warning / error / agent). Typography is the native system stack — the app should
feel like it belongs on the OS, not like a website embedded in a window.

- **Aesthetic direction:** Industrial / utilitarian developer tool with matched
  dark and light palettes
- **Decoration level:** minimal — contrast and spacing do the work, not ornament
- **Reference points:** Linear, Raycast, native macOS settings panes

## Color Palette & Roles

The persisted appearance preference applies `.dark` to `<html>` only when the
resolved scheme is dark. Users choose dark, light, or follow system from Config;
all windows resolve the same preference. Values are the canonical hex; Tailwind
exposes each as a `--color-*` utility.

### Primary

- **Indigo** (`#4f46e5`): primary CTA, active nav/scope/view selection, focus
  rings (lightened to `#6366f1` in dark for ring legibility)
- **Indigo foreground** (`#ffffff`): text/icon on indigo fills

### Interactive

- **Ring** (`#6366f1` dark / `#4f46e5` light): focus outline on all interactive
  elements (`ring-[3px]` via shadcn)
- **Hover**: `accent`/`secondary` surface lift; never a new hue

### Text

- **Foreground** (`#e6e8ee` dark / `#0e0f13` light): primary text
- **Muted foreground** (`#9aa0ad` dark / `#6b7280` light): secondary text, hints,
  uppercase section labels, monospace paths
- **Disabled**: `opacity-50` on the element (shadcn convention), no separate token

### Surface

- **Background** (`#0e0f13` dark / `#ffffff` light): app canvas
- **Card / Popover** (`#16181f` dark): panels, tables, the elevated popover step
  uses `#1d2029` dark for menus/dialogs
- **Secondary / Muted** (`#1d2029` dark / `#f2f3f5` light): inset surfaces —
  inputs, secondary buttons, kind/tree group rows, chips
- **Accent** (`#2c313d` dark / `#eef0f3` light): the hover / selection surface.
  Deliberately one step lighter than `Secondary/Muted/Popover` (all `#1d2029`
  in dark) so `bg-accent` highlights are visible on every surface. Keep it
  distinct from those tokens — collapsing them back is the hover-invisibility bug.

### States & Shadows

- **Card shadow:** none by default — elevation is color contrast. Floating
  surfaces (dropdown menu, dialog) use shadcn's `shadow-md` / `shadow-lg`.
- **Border / Input** (`#272a33` dark / `#e4e6ea` light): hairline structure
- **Theme strategy:** light and dark are matched production palettes. Light is a
  faithful inversion with reduced saturation, not a separate layout or visual
  language.

### Semantic (custom tokens beyond the shadcn set)

- **Success** `#22c55e` dark / `#16a34a` light — capability `enabled`, "all on"
- **Warning** `#f59e0b` dark / `#d97706` light — `stale` / `broken` / `foreign`
  states, scan notices
- **Destructive** `#ef4444` — delete, take-over, errors
- **Kind: Agent** `#a855f7` dark / `#9333ea` light — agent capability accent
- Kind: Skill uses **primary** (indigo); Kind: Rule uses **success** (green);
  Kind: Hook uses **warning** (amber)

Stale Manager cells use a full warning-triangle button, not an unlabeled status
dot. Its accessible menu explains the drift and offers the applicable safe
recovery (stage source refresh or re-sync the live suite binding) plus Open
source and Reveal target. Broken and foreign cells keep their existing state
treatment and never expose stale-only recovery actions.

## Typography

### Font Family

- **Display & Body:** native system UI stack — `-apple-system,
  BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif`. A
  deliberate choice: a desktop utility should render in the OS font for a native
  feel and zero font-loading cost (the app is offline-first).
- **Data / Tables:** same system stack with `tabular-nums` where counts align
- **Code / Paths:** `ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace`
  (exposed as `font-mono`)
- **Loading:** none — system fonts are always present

### Hierarchy

| Role | Font | Size | Weight | Line Height | Letter Spacing | Notes |
|------|------|------|--------|-------------|----------------|-------|
| App Title | system | 16px | 600 | 1.3 | 0.2px | Header H1 |
| Empty-state Title | system | 18px | 600 | 1.3 | 0.2px | First-run hero |
| Dialog Title | system | 16px | 600 | 1.3 | normal | Modal headings |
| Section Label | system | 12px | 600 | 1.4 | 0.08em | UPPERCASE, muted |
| Body | system | 13px | 400 | 1.5 | normal | Base size |
| Body Emphasis | system | 13px | 600 | 1.5 | normal | Names, pending count |
| Button | system | 13px | 500 | 1 | normal | shadcn `text-sm` |
| Badge / Chip | system | 11px | 500 | 1 | normal | Kind + state pills |
| Caption / Hint | system | 11–12px | 400 | 1.4 | normal | muted-foreground |
| Code / Path | mono | 11px | 400 | 1.4 | normal | Paths, relative paths |

### Principles

- One base size (13px). Step up only for titles, down only for chips/captions.
- Weight does the emphasis work: 400 body, 500 controls, 600 names/headings.
  Never bolder than 600.
- Uppercase + `0.08em` tracking + muted color is the single "section label" motif.
- Monospace is reserved for filesystem paths — it signals "this is a real path".

## Component Stylings

All components are shadcn/ui (New York style). Do not hand-roll; compose these.

### Iconography (`lucide-react`)

- **Library:** [`lucide-react`](https://lucide.dev) is the single icon source.
  Never inline raw SVG or use unicode glyphs (e.g. `▸`/`▾`) for UI affordances.
- **Sizing:** icons inherit the shadcn `[&_svg]` rules (default `size-4`; `size-3`
  in `xs`/`icon-xs` controls). Pass an explicit `size-*` only to override.
- **Color:** icons take `currentColor` — they follow the surrounding text/foreground
  token. Standalone (non-text) icons sit on `text-muted-foreground` until hovered.
- **Icon-only buttons** must carry an `aria-label` and a `Tooltip` with the action
  label (text is never dropped, only relocated to the tooltip).
- **Established icon language (Manager toolbar):**
  - Scope: `Globe` (global) · `FolderGit2` (workspace)
  - View: `List` (flat) · `FolderTree` (tree)
  - Search field affix: `Search`
  - Filters: `Tag` (type) · `Database` (source)
  - Collapse all: `ChevronsDownUp` · Expand all: `ChevronsUpDown`
  - **Tree row caret:** `ChevronRight` (collapsed) · `ChevronDown` (expanded)
  - **Accordion caret** (Config Tools): same pair — `ChevronRight` collapsed,
    `ChevronDown` expanded (not a rotated single icon)
  - Row "more actions": `MoreHorizontal`
  - Matrix cell state: `Check` (on) · `Minus` (mixed)

### Buttons (`@/components/ui/button`)

- **Primary** (`variant="default"`): indigo fill, white text — Apply, Save,
  Scaffold, Apply Suite/Patch, + New
- **Secondary/Ghost** (`variant="ghost"`): transparent, accent hover — Reset,
  Cancel, Add source, Browse, Expand/Collapse all, row-action triggers
- **Destructive** (`variant="destructive"`): red fill — Delete & take over.
  Destructive *secondary* actions (Remove source, Delete suite) use
  `variant="ghost"` with `text-destructive` to stay quiet until intended.
- **Sizes:** `default` (h-9) for footer/primary; `sm` (h-8) for in-panel;
  `xs` / `icon-xs` for table-dense controls and toggle cells.

### Cards & Containers (`@/components/ui/card`)

- Background `card`, no border by default in dark (color contrast separates from
  `background`); 12px radius; padding 16px. Used for every config panel, the
  suite/workspace panes, and the suite bar.
- No hover elevation on static panels.

### Accordion (`@/components/ui/accordion`)

- Classic single-select (`type="single"` + `collapsible`) for Config → Tools:
  one tool’s projection paths open at a time; all start collapsed.
- Keep interactive controls (e.g. enable `Switch`) **outside** `AccordionTrigger`
  so they are not nested buttons and do not toggle expand/collapse.

### Inputs & Selects (`input`, `select`, `label`)

- `secondary`/`input` surface, hairline border, 8px radius, 13px text.
- Search inputs use `type="search"`; path inputs add `font-mono`.
- Every field gets a `Label` (uppercase section-label style) for a11y.

### Toggles & Tabs (`tabs`, `toggle-group`, `toggle`, `switch`, `checkbox`)

- **Nav** (Manager / Resources / Statistics / Config): `Tabs`, indigo active
  state — the selected tab is an indigo fill with white text, never a graphite
  surface step. The selection must be unmistakable at a glance. Suites is a
  scope inside Manager (via the scope rail), not a separate nav tab.
- **View** (Flat / Tree): `ToggleGroup` (single-select), indigo active (indigo
  fill + white text on the selected item, applied at the `ToggleGroupItem` level
  so the Watch `Toggle` keeps its dot look). Items are **icon-only** (`List` /
  `FolderTree`) with a `Tooltip` and `aria-label` — no text, to keep the toolbar
  compact.
- **Scope** (Global / Suites / Workspace): a sticky left `aside` rail beside
  the Manager content pane, not a header dropdown — a single Global row, a
  Suites section (New, Create from current, then each suite as a row with a
  Base badge and capability count), and a Workspaces section (add + per-target
  rows). The active row gets a `border-primary bg-primary/10` treatment.
- **Watch pill**: `Toggle` with a status dot — green glow when watching, muted
  when paused.
- **Tool enable** (Config): `Switch`.
- **Capability selection** (Manager suite scope): `Checkbox` with native
  `indeterminate` for partial folder/kind selection in the tri-state Included
  column of the shared capability table.
- **Matrix tri-state cell**: a small `icon-xs` button — empty (off), `✓`
  (on, success tint), `–` (mixed, indigo tint); a `mod` ring marks a staged
  change; a warning dot marks an abnormal current state.

### Badges (`badge`)

- **Kind badge** (`outline`): skill=indigo, agent=kind-agent, rule=success,
  hook=warning text on a neutral pill.
- **State / count chip**: `secondary` default; `included` suite chip tints
  success.

### Table (`table`)

- The capability matrix. Sticky header row on `secondary` surface, hairline row
  separators, group rows (kind / folder) on a slightly darker inset.

### Navigation (Header)

- Solid `card` bar, hairline bottom border, 16px/24px padding. Logo + title +
  capability/source count on the left; nav tabs + watch pill on the right.
- The header is a **stable nav**: the right cluster (nav tabs + watch pill) is
  identical on every route and never reflows when switching Manager /
  Resources / Statistics / Config. The Manager's Global / Suites / Workspace
  **scope rail** lives to the left of the content pane and appears only on the
  Manager route; because it sits on the left, its show/hide leaves the right
  cluster anchored and stable.

### Dialogs (`dialog`, `alert-dialog`)

- **Dialog**: conflict take-over confirmation (destructive emphasis, lists
  blocked targets).
- **AlertDialog**: suite delete and "apply empty suite" confirmations (replaces
  `window.confirm`).

### Dropdown Menu (`dropdown-menu`)

- Per-row "more actions" (Open original, Reveal in Finder, Open in <tool>),
  replacing the hidden `<details>` menu. Trigger is a ghost `icon-xs` `⋯`
  revealed on row hover.
- **Item highlight:** highlighted / hovered items use the standard
  `bg-accent` + `text-accent-foreground` with `cursor-pointer`. This reads on the
  `popover` surface because `accent` (`#2c313d` dark) is kept distinct from
  `popover` (`#1d2029`); see the Accent surface note above.

### Toaster (`sonner`)

- Apply / suite / workspace result summaries surface as toasts (success / warning
  / error). Mounted once at the app root and follows the resolved appearance
  theme.

### Progress (`progress`)

- Apply progress (`done/total`) renders in the footer ActionBar during apply.

### Distinctive Components

- **Capability Matrix**: the signature surface — a tool-by-capability grid with
  flat (by kind) and tree (by folder) layouts, batch group toggles, and tri-state
  cells. Logic is preserved 1:1 from the pre-migration implementation.
- **Footer ActionBar**: sticky bottom bar that appears only when there are
  pending staged changes — pending count + progress on the left, Reset / Apply on
  the right.
- **Empty state**: centered card with Scaffold / Add-source affordances on first
  run.

## Layout

### Spacing

- **Base unit:** 4px
- **Density:** compact — this is a data tool
- **Scale:** 4 / 6 / 8 / 12 / 16 / 20 / 24 / 48px (common stops)

### Grid & Container

- Full-window flex column: Header (fixed) / scrollable content / ActionBar
  (conditional). Content padding 20–24px.
- Manager is a flex row: the fixed `w-64` scope rail (Global / Suites /
  Workspaces) plus a `min-w-0 flex-1` content pane rendering the shared
  capability table for every scope.

### Border Radius Scale

| Name | Value | Use |
|------|-------|-----|
| sm | 4px | toggle cells, chips, kind badges |
| md | 6px | buttons, inputs, menu items |
| lg | 8px | cards, panels, dialogs |
| pill | 999px | badges, watch pill |
| circle | 50% | status dots |

(`--radius` base = 0.5rem/8px; shadcn derives sm/md/lg from it.)

## Depth & Elevation

| Level | Treatment | Use |
|-------|-----------|-----|
| Flat (L0) | color contrast only | page background vs cards |
| Inset | darker `secondary` surface | inputs, tree group rows, chips |
| Elevated (L1) | `shadow-md` + `popover` surface | dropdown menus |
| Overlay (L2) | `shadow-lg` + scrim `bg-black/65` | dialogs |
| Focus | `ring-[3px] ring-ring/50` | all interactive elements |

Shadows appear only on truly floating surfaces (menus, dialogs). Everything else
uses surface color to imply layering — quieter and sharper on a dark canvas.

## Motion

- **Approach:** minimal-functional
- **Easing:** standard ease for hovers; shadcn/Radix defaults for enter/exit
- **Duration:** micro 120ms (hover/toggle), short 150ms (menus/dialogs)
- No decorative motion. Transitions confirm state changes; they never entertain.

## Do's and Don'ts

### Do

- Compose shadcn primitives; keep all variants token-driven.
- Use indigo for exactly one primary action per context.
- Reserve semantic color for meaning (state, kind), never decoration.
- Keep one base font size; vary weight, not family.
- Use monospace only for filesystem paths.
- Let surface contrast imply depth before reaching for a shadow.

### Don't

- Don't hand-roll buttons, inputs, modals, or menus.
- Don't introduce a second accent hue or gradients.
- Don't add borders where a surface step already separates regions.
- Don't exceed weight 600 or add letter-spacing outside section labels.
- Don't use shadows on static panels.
- Don't block apply/destructive flows without an explicit confirm surface.

## Responsive Behavior

A desktop window, but it must stay usable when narrow.

### Breakpoints

| Name | Width | Key Changes |
|------|-------|-------------|
| Narrow | < 760px | Scope rail stays fixed-width; content pane and matrix scroll horizontally rather than dropping the rail |
| Default | 760–1200px | Fixed scope rail + full matrix |
| Wide | > 1200px | Matrix breathes; content max-width not enforced |

### Touch Targets

- Primary buttons: ≥ 32px height (h-8/h-9)
- Toggle cells: 24–26px (mouse-precision dev tool; not touch-first)
- All interactive: focus ring guarantees keyboard reachability

### Collapsing Strategy

- Header actions wrap before truncating; nav tabs stay visible.
- Matrix scrolls horizontally rather than dropping tool columns.
- Long paths ellipsize; full value via `title`.

## Agent Prompt Guide

### Quick Reference

- Surfaces (dark): bg `#0e0f13`, card `#16181f`, popover/inset `#1d2029`,
  hover/selection accent `#2c313d`, border `#272a33`
- Text: fg `#e6e8ee`, muted `#9aa0ad`
- Accent: primary `#4f46e5`, ring `#6366f1`
- Semantic: success `#22c55e`, warning `#f59e0b`, destructive `#ef4444`,
  kind-agent `#a855f7`
- Radius base 8px; base font 13px system; mono for paths
- Tailwind utilities: `bg-background text-foreground bg-card bg-secondary
  text-muted-foreground border-border text-primary bg-primary text-success
  text-warning text-kind-agent`

### Example Component Prompts

- **Primary action button:**
  `<Button onClick={apply}>Apply</Button>` (indigo default).
- **Quiet destructive:**
  `<Button variant="ghost" className="text-destructive hover:text-destructive">Remove</Button>`
- **Section label + input:**
  `<Label className="text-xs uppercase tracking-[0.06em] text-muted-foreground">Name</Label>`
  then `<Input className="font-mono" />` for a path.
- **Kind badge:**
  `<Badge variant="outline" className="text-primary">skill</Badge>`
  (swap `text-kind-agent` / `text-success` / `text-warning` for agent/rule/hook).
- **Panel:**
  `<Card className="p-4"><CardHeader>…</CardHeader><CardContent>…</CardContent></Card>`

## Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-06-02 | Initial design system created | Formalize the existing dark developer-tool aesthetic into shadcn/Tailwind v4 tokens during the frontend migration |
| 2026-06-02 | System UI font as Body | Native desktop feel, offline-first, zero font-load cost; deliberate deviation from a web display font |
| 2026-06-02 | Dark-first, light secondary | The product is a dense control surface used for long sessions; dark is the design, light is a faithful inversion |
| 2026-06-02 | Custom success/warning/kind-agent tokens | Capability state and kind need stable semantic color beyond the shadcn default set |
| 2026-06-03 | Indigo active for nav/scope/view | shadcn defaults rendered active states as a near-invisible graphite step; indigo fill makes the current selection unmistakable, matching the stated palette intent |
| 2026-06-03 | Global `cursor: pointer` base rule | Tailwind v4 Preflight stopped setting pointer cursor on buttons; one base-layer rule restores it for native buttons + Radix role-based controls instead of per-component classes |
| 2026-06-03 | `lucide-react` as the icon system + Manager toolbar icon language | Formalize a consistent, scannable icon vocabulary; replace unlabeled text actions (Expand/Collapse all) and unicode carets with tooltip'd lucide icon buttons |
| 2026-06-03 | Scope switcher = header-left `Select` (not a content sub-bar) | A dedicated toggle row wasted vertical space; a compact dropdown next to the title reclaims it and, by living in the left cluster, keeps the right-side nav stable across routes |
| 2026-07-17 | App-wide appearance preference | Light, dark, and follow-system choices share one resolved token palette across every WebView and native window; the persisted preference is applied before the main window is shown |
| 2026-07-31 | Palette shortcuts as one labeled set | Hub toggle, all-resource, skill, and command accelerators use a compact label/input grid with one Save and one Reset action so atomic behavior is visible in the UI |
