---
title: "Web view CSS fixes: search width, full-height surface"
type: iteration
status: complete
author: "agent"
date: 2026-07-01
tags: []
related:
- implements: STORY-182
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes
<!-- guidance: the task breakdown; exact files, functions, and edits to make -->

Three web-view CSS bugs. All in `static/lazyspec.css`. No engine/TUI/CLI touch (web presentation only).

**Bug 1 — search bar wider than filters/rows below.**
Root cause: `input[type="search"]` (`:437`) = `max-width: var(--page-max)` + `width:100%`, box edge at page-max. Siblings `body>h1` (`:424`), `#filters` (`:457`), `#doc-list` (`:489`) = same `max-width` BUT also `padding: ... var(--page-pad)` (`--page-pad` = `clamp(16px,5vw,64px)`), so their content inset page-pad inside page-max. Search not inset → overhangs content column ≤64px/side.
Fix: inset search to same column.
```css
input[type="search"] {
  width: calc(100% - 2 * var(--page-pad));
  max-width: calc(var(--page-max) - 2 * var(--page-pad));
  /* keep margin-inline:auto + rest */
}
```

**Bug 2+3 — doc view dark below content + bg not full height.** Same defect.
Root cause: `body` (`:148`) sets `background: var(--surface)` but neither `html` nor `body` sets `min-height`; no `color-scheme` declared. Short doc → content < viewport → area below shows root/canvas, renders dark on device. Confirmed: doc-view-only + below-content-only (list page same defect, masked by tall content).
Fix: anchor surface to full height on root + declare scheme.
```css
:root { color-scheme: light dark; }
html  { min-height: 100%; background: var(--surface); }
```
Place `color-scheme` inside existing `:root` block (`:17`). Add `html` rule near `html{-webkit-text-size-adjust}` (`:144`).

## Test Plan
<!-- guidance: the checks the build phase must pass; one per acceptance criterion -->

- Manual (browser, no headless render avail): list page `/` — search field left/right edges align with filter labels + table rows at wide AND narrow viewport.
- Doc page `/doc/<short-doc>` light device: full viewport surface light, no dark strip below content.
- Doc page tall doc: unchanged.
- `cargo build --features web` clean; existing `tests/integration/web_serve_test.rs` green (no markup change → no test change).

## Notes
<!-- guidance: constraints, gotchas, and decisions that bound the implementation -->

- CSS-only. No template, route, or engine edit → TUI/CLI parity N/A (pure web styling).
- `calc()` with `clamp()` var valid; aligns on wide (capped page-max−2·pad) and narrow (100%−2·pad gutter).
- Setting `html` bg explicitly + `min-height:100%` makes the exposed-canvas color moot; bug 2 resolved regardless of why canvas read dark.
- `color-scheme: light dark` also aligns UA scrollbar/chrome to active scheme.
