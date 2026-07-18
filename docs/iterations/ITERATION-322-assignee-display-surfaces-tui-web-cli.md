---
title: 'Assignee display surfaces: TUI, web, CLI'
type: iteration
status: accepted
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-222
---

## Objective

Display assignee on all three surfaces — TUI, web view, CLI — in both list column and detail view. Feature parity across the three.

## Satisfies

STORY-222 AC5. Depends on core slice (`DocMeta.assignee` + `doc_to_json`) — this iteration `blocked-by` it.

## Context

- Story + ACs: STORY-222. `--json` field already added in core slice (`json.rs` `doc_to_json`); this slice is human/visual surfaces.
- Mirror `tags`/`status` display in each surface. `assignee` is `Option<String>` — blank/omitted when `None`.
- CLI: `src/cli/show.rs` detail header block (L117-128) — add conditional `Assignee:` line near `Author:`/`Tags:`. `src/cli/list.rs` (L24-35) + `src/cli/status.rs` (L76-81) both call `doc_card(...)`; `src/cli/style.rs` holds `doc_card` — thread assignee into the list card/column.
- TUI (`src/tui/views/panels.rs`): list column — `doc_column_width_spec` (L270-277) add `"assignee"` arm; width const region (L256-263) add `ASSIGNEE_COLS`; `doc_column_cell` (L609-637) add `"assignee"` render arm reading `doc.assignee`; `draw_doc_list` (L939-998) header labels. Default column set `default_table_columns` (`src/engine/config.rs` L523-528) — add `"assignee"` for parity. Detail: `build_preview_header_lines` (L1046-1117) add conditional `Assignee:` (near L1071 / Tags block L1075); `render_fullscreen_document` header (L1300-1328). NOTE: TUI generic `column_cell_text` fallback already renders an undeclared `assignee` attr column, but now first-class => add bespoke arm + width for parity.
- Web: `src/web/render.rs` — `DocRow` struct (L24-32) add `assignee`; `DocPage` struct (L141-171) add `assignee`; `DocPage::from_doc` (L180-271) populate. `src/web/routes.rs` — `build_groups` (L64-95) + `search` handler (L284-302) populate `DocRow.assignee`. `templates/list_row.html` (L1) add cell; `templates/doc_page.html` `<dl>` (L23-26) add `<dt>assignee</dt><dd>…</dd>`.

## Tasks

1. Test-first where apt: web view-model unit (`DocRow`/`DocPage` assignee populated from `DocMeta`); CLI `show` detail contains `Assignee:` when set; TUI `doc_column_cell` assignee text.
2. CLI detail (`show.rs` L117-128): `if let Some(a) = &doc.assignee` print `Assignee:` line — mirror `Tags` conditional (AC5 detail).
3. CLI list column: thread assignee into `doc_card` (`style.rs`) so `list.rs` (L25) + `status.rs` (L79) render assignee column; blank when `None` (AC5 list).
4. TUI list column (`panels.rs`): add `ASSIGNEE_COLS` (L256), `"assignee"` arm in `doc_column_width_spec` (L270) + `doc_column_cell` (L609) reading `doc.assignee`; header via `draw_doc_list` (L939). Add `"assignee"` to `default_table_columns` (`config.rs` L523) (AC5 list).
5. TUI detail (`panels.rs`): `build_preview_header_lines` (L1046) conditional `Assignee:` line (~L1071); `render_fullscreen_document` (L1300) header (AC5 detail).
6. Web list: `render.rs` `DocRow` (L24) + populate in `routes.rs` `build_groups` (L64) & `search` (L284); `list_row.html` add assignee cell (AC5 list).
7. Web detail: `render.rs` `DocPage` (L141) + `from_doc` (L180) populate; `doc_page.html` `<dl>` (L23) add assignee `dt`/`dd` (AC5 detail).
8. Update README: assignee now shown in CLI list + `show` detail.

## Out of scope

- AC1/AC2/AC6 field + JSON (core), AC3/AC4 remote inherit + write-through (remote slice) — blocking deps.
- Assignee filtering/sorting — STORY-222 out-of-scope.

## Principles / conventions

- CLAUDE.md: feature parity across tui/web/cli (list + detail on all three); update README on CLI change.
- Mirror `tags`/`status` display in each surface.

## Verification

- Assigned doc: appears in CLI list column, `show` detail `Assignee:` line, TUI list column + preview header, web list row + doc-page `<dl>` (AC5, all three surfaces, list + detail).
- Unassigned doc: column blank, no detail line, no crash.

