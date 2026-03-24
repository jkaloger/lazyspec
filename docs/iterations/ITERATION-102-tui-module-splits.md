---
title: TUI Module Splits
type: iteration
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-081-tui-nesting-naming-and-module-splits.md
---



Covers RFC-032 streams 5a, 5b, and 5c. Splits the three largest TUI source files into focused sub-modules. All public items are re-exported so call sites outside each module require no changes. Should follow ITERATION-101 so splits are applied against already-renamed functions.

## Changes

### 5a: Split ui.rs into ui/

- [ ] Create `src/tui/ui/` directory with `colors.rs`, `layout.rs`, `panels.rs`, and `overlays.rs`
- [ ] Move `status_color`, `tag_color` to `colors.rs`
- [ ] Move `wrapped_line_count`, `calculate_image_height` to `layout.rs`
- [ ] Move `draw_doc_list`, `render_document_preview`, `render_relationship_sections` to `panels.rs`
- [ ] Move `draw_help`, `draw_create_form`, `draw_delete_confirm`, `draw_status_picker`, `draw_link_editor`, `draw_agent_dialog`, `draw_search`, `draw_warnings` to `overlays.rs`
- [ ] Retain `draw()` and the layout dispatch logic in `ui.rs` as the module root
- [ ] Re-export all public items from `ui.rs` so external imports are unchanged

### 5b: Split app.rs into app/

- [ ] Create `src/tui/app/` directory with `forms.rs`, `cache.rs`, `keys.rs`, and `graph.rs`
- [ ] Move `CreateForm`, `DeleteConfirm`, `StatusPicker`, `LinkEditor`, `AgentDialog` structs and their methods to `forms.rs`
- [ ] Move body expansion, diagram rendering, and filtered docs cache logic to `cache.rs`
- [ ] Move `handle_key` and the key dispatch match tree to `keys.rs`
- [ ] Move `rebuild_graph` and `traverse_dependency_chain` to `graph.rs`
- [ ] Retain the `App` struct definition and core state fields in `app.rs`
- [ ] Re-export all public items from `app.rs` so external imports are unchanged

### 5c: Split gfm.rs into gfm/

- [ ] Create `src/tui/gfm/` directory with `parse.rs` and `render.rs`
- [ ] Move `extract_gfm_segments` and all extractor structs (from ITERATION-101) to `parse.rs`
- [ ] Move segment-to-`Line` conversion and styling logic to `render.rs`
- [ ] Retain the `GfmSegment` enum and re-exports in `gfm.rs` as the module root

## Test Plan

- [ ] `cargo build` passes with no warnings after each file split
- [ ] `cargo test` passes after all three splits are complete
- [ ] Confirm no `use` paths outside `ui/`, `app/`, or `gfm/` need updating (imports resolve via module roots)
- [ ] Manually smoke-test the TUI: open document list, preview, fullscreen, filter panel, link editor, and help overlay

## Notes

These splits are purely structural. Each split should be its own commit so that any import resolution errors are isolated to the relevant module. Apply 5c after ITERATION-101 is merged so the extractor structs already exist in `gfm.rs` before moving them to `gfm/parse.rs`.
