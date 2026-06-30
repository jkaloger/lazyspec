---
title: 'Web view app shell: sidebar + header, back-link, clickable tag filter'
type: iteration
status: in-progress
author: agent
date: 2026-07-01
tags: []
related:
- implements: STORY-182
---

## Objective

Web view app shell (Slack/native-app chrome): shared left sidebar nav (All / Graph / doc-types) + top header (repo·branch top-left, center search trigger that opens a Cmd-K search modal) across list+doc+graph pages. Style back-link. Make `.tag` clickable → filter list by tag. Add `type` param to list (sidebar type links target it).

## Satisfies

Net-new chrome beyond STORY-182 AC1-10 (story presentation-only; no existing AC covers app shell / nav sidebar / header / clickable tags). Follow-up to STORY-182. Graph pivot picker deferred → ITERATION-246.

## Context

- Parent story + design system: STORY-182, RFC-053 ("Color tokens" accent discipline, "Component specifications", "Type tokens" label/mono).
- Tokens + status/relation treatment already shipped: ITERATION-242 (`:root` tokens, `--accent`/`--accent-weak`, `--rule`, square lock, motion), ITERATION-243 (status swatch, relation accent-underline), ITERATION-244 (list rows, hairlines).
- Quality gates: taste-skill stack-agnostic only — accent = single interactive axis (sidebar active row + tag link + back-link + header search focus = interactive ✓), square lock radius 0, theme lock, em-dash ban, density restraint, mono `--t-label` for nav/branch chip. Internal devtool, not marketing — taste-skill landing/hero/section rules N/A. Ignore React/Tailwind/Motion.
- Engine: `Filter.doc_type` exists, `list_fragment` hardcodes `None` (`src/web/routes.rs:115`). Wiring `type` param = small. `ListQuery` = add `type`. `Store::search` exists, served by `/search` (renders `search_fragment.html`) — reuse as-is for the modal; no list_page `q` wiring needed.
- Git chrome data (resolve once at startup, add to `AppState`):
  - branch: `crate::engine::git_status::query_git_branch(store.root())` → `Option<String>`.
  - repo name: `store.root()` dir `file_name`. (`RepoCoords` is github-only, may be `None`; do not depend on it for the chip.)
- Search = modal (Cmd-K palette), global chrome → works identically on every page; no cross-page navigation hack. Header center = a trigger button (not an inline input). Click trigger or `/` / `Cmd-K` (`Ctrl-K`) → open modal overlay; `Esc` / backdrop click → close. Modal input `hx-get="/search"` targets a results pane inside the modal (`#search-results`), not page `#doc-list`. `search_fragment.html` rows already `<a href="/doc/{id}">` (via `list_row.html`) — navigable as-is. Small vanilla JS in `_search_modal.html` for open/close + key handlers (no framework).
- Existing hooks:
  - search: bare `<input type="search" name="q" hx-get="/search" ...>` in `list_page.html` only. Move into shared header partial.
  - back-link: bare `<a href="/">&larr; all documents</a>` in `doc_page.html`, `graph_page.html` — no class. Subsumed by header (back affordance = repo chip / All link); keep `.back-link` styling for the anchor if retained.
  - tags: `doc_page.html` `.doc-tags > span.tag` static (not links).
  - doc types: enumerable for sidebar (same source feeding `statuses`/`tags` selects).
- Touch:
  - `templates/_header.html` (new) — top bar: left = repo·branch chip (mono); center = search trigger button (opens modal). Include in all 3 pages.
  - `templates/_search_modal.html` (new) — hidden overlay: backdrop + centered panel, search input (`hx-get="/search"` → `#search-results`), `#search-results` pane, vanilla JS for open/close + `/`/`Cmd-K`/`Esc`. Include in all 3 pages.
  - `templates/_sidebar.html` (new) — shared nav partial: All documents (`/`), Graph (`/graph`), type list (links → `/?type={t}`). Include in all 3 pages.
  - `templates/list_page.html`, `doc_page.html`, `graph_page.html` — app-shell layout: header across top, sidebar left, `<main>` right; include header + sidebar + search-modal partials. Remove inline search input from `list_page.html`. doc/graph: `.back-link` class on back anchor; tags → `<a class="tag" href="/?tag={tag}">`.
  - `src/web/server.rs` `AppState` — add `repo_name: String`, `branch: Option<String>`; populate in startup builder.
  - `src/web/routes.rs` — `ListQuery.r#type`; `list_page` accepts `Query<ListQuery>`, maps `type`→`filter.doc_type`. `list_fragment` maps `type` too. Add `types`, `repo_name`, `branch` fields to `ListPage`/`DocPage`/`GraphPage` structs; populate per route. `/search` unchanged.
  - `src/web/render.rs` — extend page structs with shell fields.
  - `static/lazyspec.css` — append header bar + repo chip + search-trigger + modal/backdrop + sidebar block + `.back-link` + clickable `.tag`.
  - `tests/integration/web_serve_test.rs` — header + sidebar + modal markup present on 3 pages; repo·branch chip renders; `/?type=rfc` filters; `/search?q=` returns results; tag link href correct.

## Tasks

1. `server.rs`: add `repo_name: String` + `branch: Option<String>` to `AppState`; resolve in startup builder — `repo_name` from `store.root()` `file_name`, `branch` via `git_status::query_git_branch(store.root())`.
2. `routes.rs`: add `r#type` to `ListQuery`; `list_page` takes `Query<ListQuery>`. Wire `empty_to_none(query.r#type)`→`Filter.doc_type` (reuse type-string→DocType ctor) in `list_page` + `list_fragment`. Leave `/search` handler unchanged.
3. Collect distinct doc-type list (stable order) once. Add shell fields `types`, `repo_name`, `branch` to `ListPage`/`DocPage`/`GraphPage` (`render.rs`); populate per route.
4. `templates/_header.html`: top bar — left repo·branch chip (mono `--t-label`, `·` separator, no em-dash); center `<button class="search-trigger" data-open-search>` with placeholder label + `/` hint key cap.
5. `templates/_search_modal.html`: `<div class="search-modal" hidden>` = backdrop + centered panel; `<input type="search" name="q" hx-get="/search" hx-target="#search-results" hx-trigger="input changed delay:200ms, search">` + `<div id="search-results">`. `<script>`: toggle `hidden` on trigger click / `/` / `Cmd-K`/`Ctrl-K` (preventDefault), close on `Esc` + backdrop click, focus input on open. Guard `/` so it does not fire while typing in an input.
6. `templates/_sidebar.html`: nav — All documents `/`, Graph `/graph`, then types: `<a href="/?type={t}">`. Mono `--t-label`. Active via current-page hint (data attr / matched id) — accent only on active+hover.
7. App shell in all 3 page templates: header full-width top, sidebar left, `<main>` right; include header + sidebar + search-modal partials. Remove inline search input from `list_page.html`. doc/graph: `.back-link` class on back anchor; `.doc-tags` → `<a class="tag" href="/?tag={tag}">{tag}</a>`, keep comma separators.
8. CSS append:
   - header: full-width top bar, hairline `--rule` bottom divider, flat/square. Repo·branch chip mono `--ink-muted`.
   - `.search-trigger`: centered, square, hairline border, `--ink-muted` text, `--accent` border on hover/focus. Key-cap hint mono micro.
   - `.search-modal`: fixed inset overlay, scrim backdrop (theme-locked, no blur needed), centered square panel (`--rule` border, surface bg, no radius), input `--accent` focus ring. `#search-results` reuses `.search-results` list styling. `[hidden]` hides.
   - sidebar: sticky left col, `--rule` divider, mono items, `--ink-muted` default, active/hover = `--accent` (underline or `--accent-weak` wash). Square, no card.
   - `.back-link`: mono, `--ink-muted`, `--accent` on hover, arrow glyph kept.
   - `a.tag`: mono micro-label, `--accent` underline on hover (AC5 holds — text uncolored at rest).
   - `@media (max-width:768px)`: sidebar collapses above content; modal panel near-full-width.
9. Tests (`web_serve_test.rs`): header + sidebar + search-modal markup on all 3 pages; repo·branch chip text present; search trigger present; `GET /?type=rfc` → only rfc rows; `GET /search?q=<term>` → results; doc-page tag renders `href="/?tag=..."`.

## Out of scope

- Repo/branch switcher (chip is display-only this slice; switcher = future, per ask).
- Graph pivot picker (All/types/tags re-root forest, as TUI `panels.rs::draw_graph_pivot_panel`) → ITERATION-246.
- New status/relation/row styling → already ITERATION-242/243/244.
- Engine filter/search changes (`doc_type`, `Store::search` already supported).

## Verification

- Header on all 3 pages: repo·branch chip top-left, search trigger top-center.
- Click trigger (or `/` / `Cmd-K`) on any page → modal opens, input focused; typing → live results in modal; click result → navigates to doc; `Esc`/backdrop → closes. `/` does not open modal while typing in an input.
- `/?type=rfc` → only rfc docs; clearing → all.
- Click tag on doc page → `/?tag=<tag>` filtered list, rows match list styling (no shift).
- Sidebar active item is the only accent-colored nav element; chip + inactive nav are mono `--ink-muted`.
- No em-dash anywhere in chrome; corners square; one theme.

