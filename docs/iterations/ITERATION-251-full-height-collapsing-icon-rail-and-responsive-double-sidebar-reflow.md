---
title: Full-height collapsing icon rail and responsive double-sidebar reflow
type: iteration
status: draft
author: jkaloger
date: 2026-07-01
tags: []
related:
- implements: STORY-184
---

## Changes

Web-view only. Depends on ITERATION-250 (operates on the unified single sidebar). Full-height sidebar that collapses to an icon rail; responsive reflow of the doc page's three columns. RFC-053 icon-rail delta (per STORY-184).

**1 — Icons (vendored, no build step).**
Use Tabler (MIT) SVG source for `list`, `affiliate`/`hierarchy` (graph), `chevron-left` (collapse toggle). Inline the official path data directly in `_sidebar.html` (copied from library, NOT hand-drawn per icon-discipline). No new asset route — markup-inline matches embedded-CSS pattern in `assets.rs` (fonts still sandbox-blocked, so avoid binary assets). stroke-width locked 1.75; `currentColor`; `aria-hidden`.

**2 — `templates/_sidebar.html`.**
- Each View entry: icon glyph + label span. Collapse toggle button at sidebar foot (chevron), `data-sidebar-toggle`, `aria-expanded`.
- Filter entries: when collapsed show first-letter mono badge (`<span class="nav-glyph">{{e.label|first-char}}</span>`) since arbitrary user types have no icon. Compute initial in render (`SidebarEntry.glyph: String` = first char upper) to avoid template char-slicing.

**3 — `static/lazyspec.css`.**
- Tokens: `--rail-w: 220px`, `--rail-w-collapsed: 48px`, `--icon-size: 16px`.
- Full height: `.app-shell { min-height: calc(100dvh - var(--header-h)) }`; `.app-sidebar { width: var(--rail-w); height: 100%; align-self: stretch }`.
- Collapsed: `body[data-sidebar="collapsed"] .app-sidebar { width: var(--rail-w-collapsed) }` → hide `.nav-label`, show `.nav-glyph`/icon centered, hide section labels. width transition gated on motion.
- `.nav-item` becomes icon+label flex row; icon `flex:none`.
- Responsive:
  - `@media (max-width:1100px)`: force collapsed rail (`.app-sidebar` = collapsed width, labels hidden) regardless of toggle — keeps doc page `rail | 220px metadata | body` from crushing.
  - `@media (max-width:768px)` (existing block `:931`): rail → off-canvas drawer (`position:fixed; transform:translateX(-100%)`; `body[data-drawer="open"]` slides in; backdrop reusing search-modal backdrop pattern). Header gains a menu button to open it.
- `prefers-reduced-motion`: width/transform transitions off (global rule `:173` already nukes transitions; ensure no animation-only reveal).

**4 — `templates/_header.html`.** Add `data-drawer-toggle` menu button (shown only < 768px via CSS) to open the drawer.

**5 — collapse/drawer JS.** Small vanilla `<script>` (like `_search_modal.html`): toggle persists `data-sidebar` to `localStorage('ls-sidebar')`, restored on load (set before paint to avoid flash — inline in `<head>` of each page template, or `data-` attr written by tiny head script). Drawer toggle sets `body[data-drawer]`, Esc/backdrop closes. Guard `matchMedia('(prefers-reduced-motion)')` for instant vs animated.

**6 — page templates.** Add the head no-flash script + ensure `_sidebar`/`_header` include the new controls on all three pages (list, doc, graph).

## Test Plan

- `web_serve_test.rs`: sidebar markup has collapse toggle (`data-sidebar-toggle`) + View icons (inline `<svg aria-hidden`); filter entries carry `nav-glyph` first-char. Header has `data-drawer-toggle`.
- Manual (browser, no headless): wide → full-height rail, toggle collapses to 48px icon rail, persists across reload. Medium (~900px) doc page → rail auto-collapsed, 220px metadata intact, body readable. < 768px → rail hidden, menu button opens drawer, Esc/backdrop closes. Reduced-motion → no slide animation.
- `cargo test --features web` green; build clean.

## Notes

- No-flash: read `localStorage` in a head script that sets `data-sidebar` before first paint; otherwise collapsed users see an expand-then-collapse jump.
- Auto-collapse at 1100px is CSS-forced and independent of the user toggle (toggle governs >1100px only); documented so the two don't fight.
- Icons inline-vendored, not a binary asset → no `assets.rs` change, no font-style sandbox blocker.
- Engine untouched → TUI/CLI parity N/A. TUI already has a full-height pane model; this brings web to rough parity (collapsible nav) without sharing code.
