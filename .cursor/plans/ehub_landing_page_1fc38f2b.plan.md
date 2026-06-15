---
name: eHub Landing Page
overview: Build a modern, geek-styled dark landing page for "eHub" (rebranded Agentic Hub) in the e-studio-hubs Next.js app, with section components, brand tokens derived from the new logo, and clearly-marked TODO image slots for product screenshots.
todos:
  - id: branch
    content: Create feature branch feat/ehub-landing-20260614 in e-studio-hubs
    status: completed
  - id: logo
    content: Copy logo PNG to e-studio-hubs/public/ehub-logo.png
    status: completed
  - id: deps
    content: Add lucide-react dependency
    status: completed
  - id: tokens
    content: Add eHub brand tokens, dark canvas, grid/glow utilities in globals.css
    status: completed
  - id: layout
    content: Update layout.tsx metadata, favicon, force dark mode
    status: completed
  - id: imageslot
    content: Build reusable ImageSlot TODO placeholder component
    status: completed
  - id: sections
    content: Build landing section components (nav, hero, tools, problem, features, how-it-works, safety, cta, footer)
    status: completed
  - id: page
    content: Compose sections in app/page.tsx
    status: completed
  - id: verify
    content: Run next build + lint, fix issues; RELEASE entry + open MR
    status: completed
isProject: false
---

# eHub Landing Page

Build the marketing home page for **eHub** (the rebrand of Agentic Hub) in the existing Next.js web app at `e-studio-hubs`. Dark-first, geek aesthetic: deep slate canvas, purple→blue neon glow (matching the new logo), mono-font accents, subtle grid/terminal motifs. Product content is distilled from [agentic-hub/PRODUCT.md](/Users/ArnoYe/Developer/agentic-hub/PRODUCT.md). Image areas are placeholder slots marked as TODO.

## Brand

- **Name:** eHub (replace "Agentic Hub" / "Next.js" everywhere on the page)
- **Logo:** copy `agentic-hub/src/assets/ChatGPT Image Jun 14, 2026, 10_55_26 PM.png` → `e-studio-hubs/public/ehub-logo.png` (also used as favicon)
- **Palette:** near-black canvas (`#080810`), purple `#8b6cff` → blue `#4d8bff` gradient accents, zinc text, mono captions
- **Voice:** "Own your AI building blocks — skills, agents, and rules, projected across every coding tool from one hub."

## Tech approach

- Keep sections as **server components** (static, zero client JS) for performance; only the nav mobile-menu toggle is a small `"use client"` island.
- Add `lucide-react` for feature icons (Next.js optimizes the barrel import automatically).
- Reusable `ImageSlot` component renders a dashed, labeled placeholder with a visible `TODO` badge and fixed aspect ratio — drop-in for real screenshots later.
- File-size cap 600 lines/file; split the page into section components.

## Page structure (single-scroll landing)

1. **Nav** — logo + `eHub` wordmark, anchor links (Features, How it works, Suites, Safety), GitHub link, "Get eHub" CTA. Sticky, blurred, mobile menu island.
2. **Hero** — headline + subhead + dual CTA, animated grid/glow background, large `ImageSlot` for the app screenshot (TODO).
3. **Tools strip** — the five adapters as mono badges: Codex, Claude Code, Cursor, OpenClaw, OpenStandard (`~/.agents`).
4. **Problem** — the three pains from PRODUCT.md: drift between tools, no safe batch control, no scenario switching.
5. **Features** — icon grid of six: Shared-root scan, Per-tool projection, Plan-then-apply safety, Suite presets, Workspace inventory, Demo scaffold.
6. **How it works** — 3 steps (Scan → Stage → Apply) with a supporting `ImageSlot` (TODO).
7. **Safety / geek-cred** — Rust core owns the FS, never overwrites real files, atomic dotfile writes, no shell capability.
8. **CTA** — final "Get eHub" band over gradient glow.
9. **Footer** — wordmark, links, "macOS first · Linux follow-on", subtle build note.

## Files

- New: [e-studio-hubs/app/page.tsx](/Users/ArnoYe/Developer/e-studio-hubs/app/page.tsx) — compose the sections.
- Edit: [e-studio-hubs/app/layout.tsx](/Users/ArnoYe/Developer/e-studio-hubs/app/layout.tsx) — metadata (`title: "eHub — manage your agentic skills, agents & rules"`, description, icon), keep Geist + Geist Mono, force dark (`<html className="dark ...">`).
- Edit: [e-studio-hubs/app/globals.css](/Users/ArnoYe/Developer/e-studio-hubs/app/globals.css) — brand CSS variables, dark canvas defaults, grid-background + glow utilities, gradient-text helper.
- New `e-studio-hubs/components/landing/`: `nav.tsx` (+ `mobile-menu.tsx` client island), `hero.tsx`, `tools-strip.tsx`, `problem.tsx`, `features.tsx`, `how-it-works.tsx`, `safety.tsx`, `cta.tsx`, `footer.tsx`, `image-slot.tsx`.
- New asset: `e-studio-hubs/public/ehub-logo.png` (copied logo).
- Add dependency: `lucide-react`.

## Workflow (feature-dev)

- Branch first: `feat/ehub-landing-20260614` (never commit to `main`).
- Implement section-by-section, verify with `pnpm dev` / build.
- Light verification: `next build` + `next lint` clean. No test suite exists in this scaffold; ask before adding one.
- Add a one-line `RELEASE` entry and open an MR to `main` at the end.

## Out of scope

- Real product screenshots (left as TODO slots).
- Light theme / theme toggle (dark-only per decision).
- Backend, auth, download/distribution pipeline, or actual binaries behind the CTA (CTA links to a placeholder/GitHub for now).