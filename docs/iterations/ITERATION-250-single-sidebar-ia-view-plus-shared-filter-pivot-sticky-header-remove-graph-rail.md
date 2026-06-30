---
title: 'Single-sidebar IA: view plus shared filter/pivot, sticky header, remove graph rail'
type: iteration
status: draft
author: jkaloger
date: 2026-07-01
tags: []
related:
- implements: STORY-184
---

## Changes

Web-view only. Collapse two nav concepts into one sidebar; sticky header; delete graph's second rail. Depends on nothing in 249.

**Problem.** Today `_sidebar.html` = flat `All documents`, `Graph`, then type links — view-mode (Graph) conflated with filters (types). `graph_page.html` adds a 2nd left rail `.graph-pivot` duplicating type+tag list. Two sidebars, overlapping concepts.

**Target IA (one sidebar).**
```
VIEW          ← section label (mono micro-label)
  List   (/)
  Graph  (/graph)
FILTER        ← section label
  <types…>
  <tags…>
```
In List view a filter entry → `/?type=` or `/?tag=`. In Graph view → `/graph?pivot=type:` / `?pivot=tag:`. Same list, view-aware hrefs + active.

**1 — `src/web/render.rs`.**
- New `pub struct SidebarEntry { pub label: String, pub href: String, pub active: bool, pub kind: String }` (kind: `type`|`tag`).
- New `pub struct Sidebar { pub view: String, pub filters: Vec<SidebarEntry> }` (view: `list`|`graph`). `view` drives View-section active state.
- Replace per-page `types: Vec<String>` with `sidebar: Sidebar` on `ListPage`, `DocPage`, `GraphPage`. Drop `PivotRow` + `GraphPage.pivots` (folded into Sidebar).

**2 — `src/web/routes.rs`.**
- New helper `fn build_sidebar(store, view: &str, active_type: Option<&str>, active_tag: Option<&str>, active_pivot: Option<&str>) -> Sidebar`. For each type/tag emit entry; href = list-form when `view=="list"` else graph-pivot-form; active computed from the relevant active param.
- `list_page`/`list_fragment`: view="list", active from query type/tag.
- `doc_page`: view="list", no active filter.
- `graph`: view="graph", active from `pivot`. Remove `pivots` build (`:218`+); keep forest re-root logic (`:209`).

**3 — `templates/_sidebar.html`.** Rewrite: View section (List `/`, Graph `/graph`, active via `sidebar.view`), Filter section iterating `sidebar.filters` → `<a class="nav-item{% if e.active %} is-active{% endif %}" href="{{e.href}}">{{e.label}}</a>`. Section labels mono micro-labels.

**4 — `templates/graph_page.html`.** Remove `<div class="graph-layout">` + `<nav class="graph-pivot">` block (`:21`+); tree pane becomes direct child of `<main>`.

**5 — `static/lazyspec.css`.**
- Delete `.graph-layout`, `.graph-pivot`, `.graph-pivot-row`, `.graph-tree-pane` blocks (`:953`-`:1023`) + their `@media`.
- Sticky header: `.app-header { position: sticky; top:0; z-index:50 }` (`:767`).
- Sidebar sticky offset under header: `.app-sidebar { top: var(--header-h) }`; add `--header-h` token (~44px). 
- Sidebar section label style (reuse `.doc-group h2` mono-micro pattern), active `.nav-item.is-active` → accent + accent-weak (replaces `body[data-nav=...]` rules `:844`).

**6 — page templates.** `body[data-nav]` stays (used elsewhere?) — verify; View active now via `sidebar.view`, so `:844` rules removed.

## Test Plan

- `web_serve_test.rs`: `/graph` response has NO `graph-pivot`; sidebar present once. List view sidebar type entry href = `/?type=X`; graph view same entry href = `/graph?pivot=type:X`. Active entry marked `is-active` matching query. View section marks List active on `/`, Graph active on `/graph`.
- Header markup has `app-header`; (sticky is CSS, manual-verify scroll keeps header+sidebar pinned).
- `cargo test --features web` green; build clean.

## Notes

- Single sidebar = single nav source of truth; graph pivot is now "graph view + filter selected", matching TUI mental model (pivot = anchor).
- View-aware href keeps deep-links stable (`/?type=`, `/graph?pivot=type:` unchanged).
- Engine untouched (forest/anchor logic reused) → TUI/CLI parity N/A; TUI keeps its own pivot panel (native affordance), web unifies into sidebar — documented divergence, both consume same engine anchors.
