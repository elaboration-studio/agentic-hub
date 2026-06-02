# Design System — Agentic Hub

Source of truth for the Agentic Hub UI. The app is a single-user Tauri 2.x
desktop tool for projecting shared agentic capabilities across AI coding tools.
The interface is a dense, data-first control surface — it should feel like a
precise developer instrument, not a marketing page.

Implementation: tokens live in [src/index.css](src/index.css) as CSS variables
consumed by Tailwind v4 (`@theme inline`) and shadcn/ui components in
[src/components/ui/](src/components/ui/).

## Visual Theme & Atmosphere

A calm, near-black workbench where the data is the hero. Surfaces are layered in
graphite steps so structure reads through contrast rather than borders or
shadows. One confident indigo carries every primary action; semantic color is
rationed and only appears to mean something (on / warning / error / agent).
Typography is the native system stack — the app should feel like it belongs on
the OS, not like a website embedded in a window.

- **Aesthetic direction:** Industrial / utilitarian, dark-first developer tool
- **Decoration level:** minimal — contrast and spacing do the work, not ornament
- **Reference points:** Linear, Raycast, native macOS settings panes

## Color Palette & Roles

Dark is the primary theme (`.dark` on `<html>`). Light tokens are defined for
completeness but secondary. Values are the canonical hex; Tailwind exposes each
as a `--color-*` utility.

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
- **Secondary / Muted / Accent** (`#1d2029` dark / `#f2f3f5` light): inset
  surfaces — inputs, ghost-hover, kind/tree group rows, chips

### States & Shadows

- **Card shadow:** none by default — elevation is color contrast. Floating
  surfaces (dropdown menu, dialog) use shadcn's `shadow-md` / `shadow-lg`.
- **Border / Input** (`#272a33` dark / `#e4e6ea` light): hairline structure
- **Dark mode strategy:** dark is the design; light is a faithful inversion with
  reduced saturation, not a separate look.

### Semantic (custom tokens beyond the shadcn set)

- **Success** `#22c55e` dark / `#16a34a` light — capability `enabled`, "all on"
- **Warning** `#f59e0b` dark / `#d97706` light — `stale` / `broken` / `foreign`
  states, scan notices
- **Destructive** `#ef4444` — delete, take-over, errors
- **Kind: Agent** `#a855f7` dark / `#9333ea` light — agent capability accent
- Kind: Skill uses **primary** (indigo); Kind: Rule uses **success** (green);
  Kind: Hook uses **warning** (amber)

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

### Inputs & Selects (`input`, `select`, `label`)

- `secondary`/`input` surface, hairline border, 8px radius, 13px text.
- Search inputs use `type="search"`; path inputs add `font-mono`.
- Every field gets a `Label` (uppercase section-label style) for a11y.

### Toggles & Tabs (`tabs`, `toggle-group`, `toggle`, `switch`, `checkbox`)

- **Nav** (Manager / Suites / Config): `Tabs`, indigo active state.
- **Scope** (Global / Workspace) and **View** (Flat / Tree): `ToggleGroup`
  (single-select), indigo active.
- **Watch pill**: `Toggle` with a status dot — green glow when watching, muted
  when paused.
- **Tool enable** (Config): `Switch`.
- **Capability selection** (Suites): `Checkbox` with native `indeterminate` for
  partial folder/kind selection.
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
  capability/source count on the left; nav tabs, scope toggle, watch pill on the
  right.

### Dialogs (`dialog`, `alert-dialog`)

- **Dialog**: conflict take-over confirmation (destructive emphasis, lists
  blocked targets).
- **AlertDialog**: suite delete and "apply empty suite" confirmations (replaces
  `window.confirm`).

### Dropdown Menu (`dropdown-menu`)

- Per-row "more actions" (Open original, Reveal in Finder, Open in <tool>),
  replacing the hidden `<details>` menu. Trigger is a ghost `icon-xs` `⋯`
  revealed on row hover.

### Toaster (`sonner`)

- Apply / suite / workspace result summaries surface as toasts (success / warning
  / error). Mounted once at the app root, hardcoded `theme="dark"`.

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
- Suites page is a two-column grid `minmax(240px,1fr) minmax(380px,2fr)`,
  collapsing to one column under 760px.

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
| Narrow | < 760px | Suites page collapses to single column |
| Default | 760–1200px | Two-pane Suites, full matrix |
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
  border `#272a33`
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
