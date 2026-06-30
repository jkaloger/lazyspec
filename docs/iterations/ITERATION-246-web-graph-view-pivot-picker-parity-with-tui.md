---
title: Web graph view pivot picker (parity with TUI)
type: iteration
status: in-progress
author: agent
date: 2026-07-01
tags: []
related:
- implements: STORY-182
---

## Objective

Web graph view pivot picker, mirroring TUI: left pivot column listing All / doc-types / tags, re-roots forest on selection. Reuses engine `resolve_forest(store, Some(anchor))` + `resolve_forest_by_tag`. Route param + sidebar markup + CSS. No new engine logic.

## Satisfies

Net-new chrome beyond STORY-182 AC1-10 (parity with TUI graph pivot, ITERATION-208). Follow-up to STORY-182. Independent of ITERATION-245; shares only page-shell CSS conventions.

## Context

- Parent story + design system: STORY-182, RFC-053 ("Component specifications" Graph page, "Color tokens", "Type tokens").
- TUI reference (match grammar + flat order): `src/tui/views/panels.rs:785 draw_graph_pivot_panel`; `src/tui/state/app.rs` `GraphAnchor` enum (All / Type(idx) / Tag(idx)), flat order = All, types…, tags… (`anchor_to_flat`/`flat_to_anchor`), `graph_pivot_*` methods.
- Engine (reuse, do NOT reimplement): `src/engine/context.rs:197 resolve_forest(store, Option<&str>)`, `:219 resolve_forest_by_tag(store, tag)`. Current web `graph` route (`src/web/routes.rs:157`) calls `resolve_forest(&store, None)` → All.
- Tokens + tree styling already shipped: ITERATION-242 (tokens), ITERATION-244 (`.graph-tree`, `data-depth` guide lines, `.graph-type`/`.graph-status`/`.graph-related`).
- Quality gates: taste-skill stack-agnostic — accent only on active pivot row + hover, square lock, theme lock, density restraint, em-dash ban.
- Touch:
  - `src/web/routes.rs` — `GraphQuery { pivot: Option<String> }` on `graph` route; parse `pivot` → anchor (`""`/absent=All, `type:{t}`→`resolve_forest`, `tag:{t}`→`resolve_forest_by_tag`); collect type+tag lists for picker; mark active anchor.
  - `GraphPage` struct — add `pivots` (rows: label, href, active) + keep `roots`.
  - `templates/graph_page.html` — left `.graph-pivot` nav (All + types + tags as links `/graph?pivot=...`), active row flagged; tree on right.
  - `static/lazyspec.css` — append `.graph-pivot` picker block.
  - `tests/integration/web_serve_test.rs` — pivot lists All+types+tags; `/graph?pivot=type:rfc` re-roots (rfc roots only); active row marked.

## Tasks

1. `routes.rs`: add `GraphQuery` with `pivot`; `graph` handler parses prefix (`type:` / `tag:` / none) → call matching engine fn. Keep `flatten_forest` + `GraphTreeNode::nest` unchanged.
2. Build pivot rows: All (`/graph`), each doc-type (`/graph?pivot=type:{t}`), each tag (`/graph?pivot=tag:{t}`) — same flat order as TUI (All, types, tags). Set `active` on the row matching current `pivot`.
3. `GraphPage` struct: add `pivots: Vec<PivotRow>` (label, href, active, kind). Populate in handler.
4. `graph_page.html`: render `.graph-pivot` left column — All / types / tags links, mono `--t-label`; active row = `--accent` (wash or underline). Existing `.graph-tree` becomes right column. Keep back-link.
5. CSS `.graph-pivot`: sticky left col, hairline `--rule` divider, mono items `--ink-muted`, active/hover `--accent`/`--accent-weak`, square flat no-card; mobile `@media (max-width:768px)` stacks picker above tree.
6. Tests: picker emits All+at-least-one-type+at-least-one-tag rows; `/graph?pivot=type:rfc` tree roots are rfc; `/graph?pivot=tag:{t}` re-roots by tag; active row carries marker.

## Out of scope

- Sidebar global nav + clickable tags + back-link styling → ITERATION-245.
- Graph tree node styling (depth/guide/type/status/related) → already ITERATION-244.
- Interactive sort control (RFC: static view, no sort) — out.
- Engine re-root logic (already exists).

## Verification

- `/graph` (no param) = full forest (unchanged).
- `/graph?pivot=type:rfc` = forest re-rooted on rfc; `/graph?pivot=tag:foo` = tag forest. Matches TUI pivot output ordering.
- Only active pivot row is accent-colored.

