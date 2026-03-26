---
title: TUI Nesting, Naming, and Module Splits
type: story
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: RFC-032
---




## Context

This story covers RFC-032 streams 2d, 2e, 4a, 4c, 5a, 5b, and 5c. It addresses flattening of overly nested functions in the GFM and preview rendering pipeline, renaming draw_* functions to more accurately reflect their purpose, and splitting large source files (ui.rs, app.rs, gfm.rs) into focused sub-modules.

## Acceptance Criteria

### 2d: Flatten extract_gfm_segments()

- Given `extract_gfm_segments()` exists as a monolithic function,
  when the refactor is applied,
  then `TableExtractor`, `AdmonitionExtractor`, and `FootnoteExtractor` structs each exist, each implementing the `GfmExtractor` trait, and `extract_gfm_segments()` delegates to them.

- Given a document with GFM tables, admonitions, and footnotes,
  when segments are extracted,
  then each extractor produces the same output as the original monolithic implementation.

### 2e: Flatten draw_preview_content()

- Given `draw_preview_content()` exists as a monolithic function,
  when the refactor is applied,
  then `render_markdown_segment()` and `render_diagram_overlay()` exist as standalone functions and `draw_preview_content()` delegates to them.

- Given a document with markdown segments and diagram overlays,
  when the preview is rendered,
  then the output is identical to the original implementation.

### 4a: Rename draw_* functions

- Given the function `draw_preview_content` exists,
  when the rename is applied,
  then the function is named `render_document_preview` and all call sites are updated.

- Given the function `draw_relations_content` exists,
  when the rename is applied,
  then the function is named `render_relationship_sections` and all call sites are updated.

- Given the function `draw_fullscreen` exists,
  when the rename is applied,
  then the function is named `render_fullscreen_document` and all call sites are updated.

- Given the function `draw_filters_mode` exists,
  when the rename is applied,
  then the function is named `render_filter_panel` and all call sites are updated.

### 4c: Rename walk() and extract to standalone function

- Given `walk()` exists as a method,
  when the refactor is applied,
  then a standalone function named `traverse_dependency_chain()` exists, the original method is removed, and all call sites reference the standalone function.

- Given a dependency graph,
  when `traverse_dependency_chain()` is called,
  then it produces the same traversal result as the original `walk()` implementation.

### 5a: Split ui.rs into ui/

- Given `ui.rs` contains approximately 1890 lines,
  when the split is applied,
  then a `ui/` directory exists containing `colors.rs`, `layout.rs`, `panels.rs`, and `overlays.rs`, with a `mod.rs` or `lib.rs` re-exporting all public items.

- Given existing consumers of `ui.rs` exports,
  when the split is applied,
  then all import paths resolve without changes to call sites outside the `ui/` module.

### 5b: Split app.rs into app/

- Given `app.rs` contains approximately 2563 lines,
  when the split is applied,
  then an `app/` directory exists containing `forms.rs`, `cache.rs`, `keys.rs`, and `graph.rs`, with a `mod.rs` or `lib.rs` re-exporting all public items.

- Given existing consumers of `app.rs` exports,
  when the split is applied,
  then all import paths resolve without changes to call sites outside the `app/` module.

### 5c: Split gfm.rs into gfm/

- Given `gfm.rs` contains approximately 700 lines,
  when the split is applied,
  then a `gfm/` directory exists containing `parse.rs` and `render.rs`, with a `mod.rs` or `lib.rs` re-exporting all public items.

- Given existing consumers of `gfm.rs` exports,
  when the split is applied,
  then all import paths resolve without changes to call sites outside the `gfm/` module.

## Scope

### In Scope

- Flattening `extract_gfm_segments()` into extractor structs implementing `GfmExtractor`
- Flattening `draw_preview_content()` into `render_markdown_segment()` and `render_diagram_overlay()`
- Renaming `draw_*` functions to `render_*` equivalents across all call sites
- Renaming `walk()` to `traverse_dependency_chain()` and extracting to a standalone function
- Splitting `ui.rs`, `app.rs`, and `gfm.rs` into sub-module directories

### Out of Scope

- Behavioural changes to any of the refactored functions
- Changes to public API surface beyond renames covered in 4a and 4c
- New features or additional rendering logic
