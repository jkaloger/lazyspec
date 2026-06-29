---
title: Apply the Neo-Swiss design system to the web view
type: story
status: in-progress
author: unknown
date: 2026-06-30
tags: []
related:
- implements: RFC-053
---<\!-- intent: one thin slice of user-observable value, testable -->

## Story

As a reader of the lazyspec web view, I open any surface (document, list, search, graph) and see a legible Neo-Swiss / industrial-information layout instead of unstyled markup, so I can scan a document's identity and lineage at a glance.

Implements [[RFC-053]]. This is presentation-only polish on the surfaces already served by `src/web/`; it styles existing semantic class hooks and does not add user capability.

## Scope

One static stylesheet served by the web binary (no build step, no Node, no framework — per RFC-053 and ADR-001), wired into the templates, applying the full token system and component specs from RFC-053:

- `:root` token block: color (surface/ink ramp, locked vermilion accent, categorical status legend), type (sans + mono, modular scale), space (4px base), rule/shape (square corners, hairlines, no cards/shadows), motion.
- Document page: asymmetric two-column grid (narrow metadata `<dl>`, 68ch body), display title, label-first status, mobile single-column collapse.
- List + search + filter: tabular rows, search input, bare filter selects, HTMX fragment row-parity so swaps are seamless.
- Graph tree: `data-depth`-driven indentation with guide lines, per-node type/status/related treatment.
- Theming: light + dark via token override (`prefers-color-scheme` + `[data-theme]` hook), WCAG AA for body/metadata.
- Typefaces: a system neo-grotesque + monospace stack is the fallback chain; one self-hosted grotesque and one self-hosted mono are embedded in the binary at compile time (RFC-053 ADR decision 2, resolved compile-time-embed to hold the single-binary line of ADR-001) and served via `@font-face` with `font-display: swap`.

Out of scope (RFC non-goals): markup surgery beyond the shared `<head>` wiring; any JS framework / CSS build pipeline; graph canvas/SVG.

## Acceptance criteria

1. Stylesheet served and wired
   - Given the web server is running, When I GET a document, list, or graph page, Then the response `<head>` links the stylesheet and the stylesheet is served by the binary with a 200.

2. Tokens defined once, themeable
   - Given the rendered page, Then a `:root` custom-property block defines color/type/space/rule/motion tokens, and a `prefers-color-scheme: dark` (and `[data-theme="dark"]`) override redefines the color tokens with no component referencing a literal color.

3. Document page hierarchy
   - Given a document page, Then the title renders at display scale spanning full width, `.doc-frontmatter` renders as a left metadata `<dl>` (mono uppercase labels), `.doc-body` is capped at a 68ch ragged-right measure, and `.doc-status` renders label-first with a leading swatch (not a filled pill).
   - Given a viewport below 768px, Then the layout collapses to a single column with metadata stacked above body.

4. Status encoded label-first, color-redundant
   - Given any surface showing a status, Then the status label is always present in mono, color is bound only to that label, and `rejected`/`superseded` read as terminal without relying on hue (strike or reduced opacity).

5. Accent discipline (single interactive color axis)
   - Given any surface, Then the vermilion `--accent` appears only on interactive or wayfinding affordance (link underline, focus ring, active nav, hovered/active row); no status, type label, heading, or static element is colored with the accent. Any other accent-colored element is a defect.

6. List, search, filter parity
   - Given the list page, Then rows are a tabular list separated by hairlines (no cards), with id / title / right-aligned status, and row hover washes `--accent-weak`.
   - Given a search or filter HTMX swap, Then the swapped fragment rows are visually identical to the list rows (no layout shift).

7. Graph tree
   - Given the graph page, Then nodes indent by `data-depth` with a per-level guide line, `.graph-type` renders as an uncolored mono label, `.graph-status` as the status micro-label + swatch, and relation targets as accent-underlined mono ids.

8. Flat industrial surface
   - Given any surface, Then there are no card containers and no drop shadows; grouping is hairline rules + whitespace, and every corner is square (radius 0).

9. Motion restraint
   - Given `prefers-reduced-motion: reduce`, Then HTMX swap and hover transitions collapse to instant.

10. Fonts embedded and served
    - Given the default load, Then text renders on the system neo-grotesque + monospace fallback stack with no layout dependency on a network fetch.
    - Given the binary, Then one grotesque and one mono are embedded at compile time and served via `@font-face` with `font-display: swap`, and the `@font-face` `font-family` lists keep the system fallback chain.

## NFR

- Single binary, no Node, no build step (ADR-001): the stylesheet and the embedded font assets are served by the existing binary.
- WCAG AA contrast for body and metadata in both themes; the `--ink-muted` @ 13px and dark-mode status-hue parity are explicit gates, not afterthoughts.

## Notes

The shared `<head>` partial is the one place this touches markup (RFC-053 flags this against its own zero-markup non-goal); keep it scoped to stylesheet + font-preload wiring.
