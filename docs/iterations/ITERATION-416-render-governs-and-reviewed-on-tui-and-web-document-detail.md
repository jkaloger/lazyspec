---
title: Render governs and reviewed on TUI and web document detail
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-269
---

## Objective

`governs` and `reviewed` render on document detail in the TUI and the web view.

## Satisfies

STORY-269 AC1, AC2, AC6.

## Context

- Story + ACs: STORY-269
- No new screen, column or graph mark: RFC-068 §Non-goals, §Design "Surfaces"
- Fields land in ITERATION-407.
- Touch:
  - `src/tui/views/panels.rs:1075` `build_preview_header_lines` -- where type, status, author, date and tags render. Tags at `:1104` are the omit-when-empty pattern AC6 wants.
  - `templates/doc_page.html:20` the `<dl class="doc-frontmatter">`. `tags` (`:27`) is the `{% if !x.is_empty() %}` pattern; `assignee` (`:25`) is the `Option` pattern for `reviewed`.
  - `src/web/routes.rs:347` `doc_page` -- the askama template struct gains both fields.
  - `src/web/assets.rs` stylesheet only if the new `<dd>` needs a rule; reuse an existing class if one fits.
- `reviewed` renders as authored. No shortening, no forge link, no age. Anything derived from it is RFC-069.

## Tasks

1. Test-first in the `panels.rs` test module: header lines carry the globs and the sha when set, and neither label when unset (AC1, AC6).
2. Implement in `build_preview_header_lines`.
3. Test-first in the web integration tests: `doc_page` HTML carries both `<dt>`s when set and neither when unset (AC2, AC6).
4. Add the fields to the `doc_page` template struct and the `<dl>`.

## Out of scope

- File-path search -- ITERATION-417.
- Validation panels. The `governs-*` findings ride the existing one; STORY-267 and STORY-268 own them.
- Graph marks, list columns.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-007: views read state, they do not mutate it.

## Verification

`cargo run --quiet -- web` and the TUI both show the two rows on a pinned document and neither row on an unpinned one.
