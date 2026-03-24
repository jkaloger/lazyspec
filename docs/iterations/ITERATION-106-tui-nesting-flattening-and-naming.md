---
title: TUI Nesting Flattening and Naming
type: iteration
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-081-tui-nesting-naming-and-module-splits.md
---



Covers RFC-032 streams 2d, 2e, 4a, and 4c. Flattens the GFM extraction and preview rendering pipelines, and renames TUI functions whose names don't reflect their scope. No behaviour changes.

## Changes

### 2d: Flatten extract_gfm_segments()

- [ ] Define `GfmExtractor` trait with `try_start`, `feed`, and `try_end` methods in `gfm.rs`
- [ ] Implement `TableExtractor` struct with table parsing state, implementing `GfmExtractor`
- [ ] Implement `AdmonitionExtractor` struct with admonition parsing state, implementing `GfmExtractor`
- [ ] Implement `FootnoteExtractor` struct with footnote parsing state, implementing `GfmExtractor`
- [ ] Rewrite `extract_gfm_segments()` main loop to delegate to each extractor, removing inline state variables

### 2e: Flatten draw_preview_content()

- [ ] Extract `render_markdown_segment(f, area, lines, scroll)` from the markdown arm of `draw_preview_content()`
- [ ] Extract `render_diagram_overlay(f, area, image, y_offset)` from the image arm of `draw_preview_content()`
- [ ] Reduce `draw_preview_content()` match arms to one-line dispatches to the extracted functions

### 4a: Rename draw_* functions

- [ ] Rename `draw_preview_content` to `render_document_preview` and update all call sites
- [ ] Rename `draw_relations_content` to `render_relationship_sections` and update all call sites
- [ ] Rename `draw_fullscreen` to `render_fullscreen_document` and update all call sites
- [ ] Rename `draw_filters_mode` to `render_filter_panel` and update all call sites

### 4c: Rename walk() and extract to standalone function

- [ ] Extract the nested `walk()` function from `rebuild_graph` into a standalone `traverse_dependency_chain()` function
- [ ] Update `rebuild_graph` and any other call sites to reference `traverse_dependency_chain()`
- [ ] Remove the original nested `walk()` definition

## Test Plan

- [ ] `cargo build` passes with no warnings after each stream
- [ ] `cargo test` passes after all changes are applied
- [ ] Manually open a document with a GFM table, admonition, and footnote in the TUI and verify the preview renders correctly
- [ ] Manually open a document with a diagram overlay and verify image positioning is unchanged
- [ ] Verify the filter panel, fullscreen view, relations panel, and document preview all render as before the renames
- [ ] Verify dependency chain traversal in the TUI graph view produces the same results as before

## Notes

Streams 2d and 4a interact: `draw_preview_content` is renamed as part of 4a, so the extracted helpers from 2e should already use the new name `render_document_preview` by the time 4a is applied. Apply 2d and 2e before 4a to avoid a double-rename.
