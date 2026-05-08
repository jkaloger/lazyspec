---
title: Expandable document list rows
type: iteration
status: accepted
author: agent
date: 2026-05-07
tags: []
related:
- implements: STORY-115
---



## Changes

1. **Engine: MultiLineConfig** [AC4]
   - File: `src/engine/config.rs`
   - Add struct `MultiLineConfig { max_expanded_height: usize }` w/ `Default = 5`.
   - `derive(Debug, Clone, Serialize, Deserialize)`. `#[serde(default)]` on field.
   - Add field `pub multiline: MultiLineConfig` to `UiConfig` w/ `#[serde(default)]`.
   - Verify: `cargo build`. `Config::default().ui.multiline.max_expanded_height == 5`.

2. **TUI state: `wrap_mode: bool` on App** [AC1, AC2, AC3]
   - File: `src/tui/state/app.rs`
   - Add `pub wrap_mode: bool` to `App` (default `false`); init in `App::new` + test fixtures.
   - No per-row state, no clear-on-rebuild needed (mode is global).
   - Verify: `cargo build`. Unit test default + survives `build_doc_tree`.

3. **TUI keybinding: `x` toggles wrap_mode** [AC1, AC2]
   - File: `src/tui/views/keys.rs`
   - In `handle_normal_key` match, add arm `(KeyCode::Char('x'), _) => { self.wrap_mode = !self.wrap_mode; }`.
   - Toggle works regardless of selection; no `selected_doc_meta` guard.
   - `e` arm unchanged.
   - Verify: integration test pressing `x` flips `app.wrap_mode`.

4. **TUI render: wrap selected row when wrap_mode on** [AC1, AC3, AC5, AC6]
   - File: `src/tui/views/panels.rs`
   - `doc_table_widths`: 7 columns (gutter, tree, id, title-Fill, status, tags, provenance). No indicator column.
   - `DocCellWidths::from_area_width`: build via `Layout::default().direction(Horizontal).spacing(1).constraints(doc_table_widths()).split(...)`, read rect widths for title (idx 3), tags (idx 5), provenance (idx 6). Mirrors ratatui Table layout including `column_spacing`.
   - Helper `wrap_to_lines(text, width, style)`: split on `\n`, then `textwrap::wrap` each segment.
   - Helper `tag_wrapped_lines(tags, width, dim)`: greedy-pack `[name]` tokens into `Line`s, never splitting a tag, preserve per-tag color via `Span::styled`.
   - Helper `row_content_lines`: `max(title_wrap, tag_wrap, provenance_wrap)` for the natural height of full content.
   - `doc_row_for_node`: `let expanded = app.wrap_mode && index == app.selected_doc;`. When `expanded`, replace title/tags/provenance cells with multi-line wrapped versions; row height = `min(content_lines, max_expanded_height).max(1)`.
   - `render_filter_panel`: drop indicator cell from filter rows.
   - Verify: `cargo build`. Manual: long title row with `wrap_mode` on shows wrapped title; >3 tags render across lines.

5. **Help screen update** [AC1]
   - File: `src/tui/views/overlays.rs` — `x: Toggle wrap mode (selected row wraps content)`.
   - Verify: `?` shows new binding text.

6. **Validate** [all ACs]
   - `cargo test --all`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo run`, exercise all ACs manually.
   - `lazyspec validate --json`.

## Test Plan

Per DICTUM-004: behavioral, isolated, deterministic, real types where possible.

- **AC1, AC2 (wrap_mode toggle):** integration test in `tests/tui_handle_key_test.rs`. Given `App` w/ `wrap_mode == false`, dispatch `KeyCode::Char('x')` via `App::handle_key`; assert `wrap_mode == true`. Press again → `false`.
- **AC1 toggle without selection:** dispatch `x` on empty App (no docs); assert `wrap_mode == true` (no selection guard).
- **Editor preserved (regression):** dispatch `KeyCode::Char('e')` on selected row → `editor_request` set, `wrap_mode` unchanged.
- **AC4 (config):** integration test — load TOML w/ `[tui.multiline] max_expanded_height = 3`. Assert deserialised `Config.ui.multiline.max_expanded_height == 3`. Simulate row-height clamp logic: `(content_lines as u16).min(max).max(1) == 3` when content_lines=10.
- **AC5 (tags wrap):** unit test `expanded_row_cells_render_full_tag_list`. Given 6 tags, narrow tag width, dispatch `doc_row_cells_expanded`; assert tags cell debug contains every tag and no `+N` counter.
- **AC5 (tag wrap line count):** unit test `tag_wrapped_lines_multi_line_when_overflow`. 6 tags @ width 12 → `lines.len() > 1`.
- **AC6 (title wrap line count):** unit test `row_content_lines_soft_wraps_long_title`. Long title @ narrow width → `> 1` lines.
- **DocCellWidths matches ratatui layout:** unit test `doc_cell_widths_resolves_title_from_area` + `_title_scales_with_area`. Title width > 0, < area, scales monotonically with area.
- **wrap_mode survives doc_tree rebuild:** unit test on `App`. Set `wrap_mode = true`; call `build_doc_tree`; assert still `true` (global mode, not per-index).

Out of test scope: ratatui frame snapshot rendering. Verify visually in `cargo run`.

## Notes

- `e` key conflict resolved → `x` for wrap mode. RFC-040 + STORY-115 amended.
- Tree-node expansion (`▶`/`▼` via Space on parent docs) is separate from row-content wrap. Different state, different mechanism.
- `textwrap` crate chosen for soft-wrap (Rust ecosystem norm per principle 5). Word boundary default; no config knob (per principle 6).
- Wrap mode global, not per-row → no `ExpandedRows` HashSet, no clear-on-rebuild plumbing, no stale-index risk.
- Visual indicator column dropped: wrap mode is global UI affordance, individual rows don't need a per-row hint. Reduces table column count from 8 to 7.
- `MultiLineConfig` reduced to `max_expanded_height` only after indicator removal. `indicator_collapsed` / `indicator_expanded` fields and their default fns dropped per principle 6.
- Future RFC-040 section 2 (STORY-117) will reuse `textwrap` dep for table cells.
