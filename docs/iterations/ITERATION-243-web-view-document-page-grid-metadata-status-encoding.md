---
title: 'Web view document page: grid, metadata, status encoding'
type: iteration
status: in-progress
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-182
- blocks: ITERATION-244
---

<\!-- intent: one session-sized slice handed to a coding agent -->

## Objective

Style the document page against existing `.doc-*` hooks: asymmetric two-column grid, display title, metadata `<dl>`, label-first color-redundant status, accent discipline, mobile single-column collapse. CSS only on tokens from Iter A.

## Satisfies

STORY-182 AC3, AC4, AC5. List/search/graph deferred (Out of scope).

## Context

- Story + AC text: STORY-182
- Component spec, status legend, accent rule: RFC-053 sections "Component specifications" (Document page), "Color tokens" (status legend + accent discipline), "Type tokens" (display/label scale, measure)
- Tokens already defined: ITERATION-242 (`static/lazyspec.css` `:root`). This slice adds component rules only, references tokens, never literals.
- Quality gates: taste-skill stack-agnostic gates — accent = single interactive axis (AC5), square-shape lock (AC8 holds), theme lock, em-dash ban. Ignore React/Tailwind parts.
- Existing markup hooks: `templates/doc_page.html` (`.doc-frontmatter`, `<dl>`, `.doc-id`/`.doc-type`/`.doc-status`/`.doc-author`, `<h1>`, body region, `.relation`/parent/children links)
- Touch:
  - `static/lazyspec.css` — append document-page component block (grid, title, dl, status swatch, body measure, relation links, mobile `@media (max-width: 768px)` collapse)
  - `templates/doc_page.html` — only if a required hook is missing (e.g. `.doc-body` wrapper, status needs a swatch span); add minimal class/wrapper, no restructure
  - `tests/integration/web_serve_test.rs` — assert doc page emits the expected hooks/classes

## Tasks

1. Read `templates/doc_page.html`; confirm hooks vs RFC component spec. Add only missing wrappers/classes (e.g. `.doc-body`, status swatch span) via template edit.
2. CSS: asymmetric grid `--col-meta` + `--col-body`, `<h1>` at `--t-display` spanning full width with `--rule-strong` underline.
3. CSS: `.doc-frontmatter` as left `<dl>`, `--t-label` mono uppercase `<dt>` in `--ink-muted`, `--t-meta` mono `<dd>`.
4. CSS: `.doc-status` label-first, leading swatch (1px ring for in-progress vs solid complete), strike/opacity for rejected+superseded (AC4 hue-independent terminal read). `.doc-type` mono label, no color.
5. CSS: `.doc-body` capped 68ch ragged-right, baseline rhythm; flat headings/code/blockquote (hairline accents, no cards/shadow).
6. CSS: `.relation`/parent/children — mono, `--accent` underline on target id only, `--ink-muted` relation prefix. Audit: accent appears only on interactive/wayfinding (AC5).
7. CSS: `@media (max-width: 768px)` single-column, metadata stacked above body.
8. Tests: doc page response contains grid container + status swatch markup.

## Out of scope

- List/search/filter rows → AC6 (Iter C).
- Graph tree → AC7 (Iter C).
- Token/theming/font/base — owned by ITERATION-242.

## Principles / conventions

- docs/convention (layering, `--json`, Rust idioms).
- taste-skill quality gates as CSS review bar (accent discipline is AC5, a hard gate).
- writing-iterations: pointer + tasks, no restated spec.

## Verification

- GET doc page: two-column grid desktop; <768px single column.
- Every status label present in mono; rejected/superseded readable greyscale.
- Grep component CSS: `--accent` only on link/focus/hover, never status/type/heading.
