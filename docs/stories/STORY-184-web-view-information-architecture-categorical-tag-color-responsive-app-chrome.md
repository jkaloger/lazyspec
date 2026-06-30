---
title: 'Web view: information architecture, categorical tag color, responsive app chrome'
type: story
status: draft
author: jkaloger
date: 2026-07-01
tags: []
related:
- implements: RFC-053
---

## Story

As reader of lazyspec web view, want native-app nav + scannable list + responsive chrome, so I navigate docs by view+filter without double sidebars, see tags/status color in list, use on narrow screens.

Builds on STORY-182 (Neo-Swiss applied). Extends RFC-053 design system (see Design-system deltas) — amendment proposed, not yet applied.

## Acceptance Criteria

- List rows show status color (swatch + `data-status`, today bare text) AND tags.
- Tags color-coded: deterministic desaturated hue per tag, label-first (text/underline color, no decorative dot). Distinct from accent + status hues.
- One sidebar only. View section (List / Graph) above shared type+tag list. List view → entries filter; Graph view → entries pivot. `.graph-pivot` second rail removed.
- Header sticky; sidebar full-height, collapsible to icon rail (toggle persisted). Arbitrary user types → first-letter mono badge when collapsed.
- Responsive: doc page (rail | 220px metadata | body) reflows at medium width (rail auto-collapses to icons, metadata stays); < 768px rail = drawer. `prefers-reduced-motion` honored.
- Web-view only. Engine/TUI/CLI untouched.

## Design-system deltas (extend RFC-053)

RFC-053 (accepted) rules extended:
1. "Status is the single colored data dimension; type mono-only; accent reserved." → ADD tag categorical color axis. Justification: tags are user-defined data dimension like status; desaturated hues coexist with mono base, distinct from vermilion accent + status legend; label-first → legible monochrome/color-blind. Type stays mono-only.
2. No nav-chrome icon spec. → ADD icon-rail tokens (collapsed/expanded rail width, icon size/stroke). Icons = vendored MIT SVG (Tabler/Phosphor), never hand-rolled. Vanilla JS only (parity with existing search-modal JS), no build step (ADR-001 preserved).

Action: amend RFC-053 once approved.

## Iterations

- ITERATION-249: list rows carry tags+status color; deterministic tag hue.
- ITERATION-250: single-sidebar IA (View + shared filter/pivot); sticky header; remove graph-pivot rail.
- ITERATION-251: full-height collapsing icon rail; responsive double-sidebar + doc-metadata reflow.

Order: 249 independent; 250 before 251.
