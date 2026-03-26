---
title: TUI Filters View
type: spec
status: draft
author: jkaloger
date: 2026-03-25
tags: [tui, filters, querying]
related:
- related-to: docs/stories/STORY-013-filters-mode.md
---


## Summary

The Filters view provides status and tag filtering for the document store. Entering Filters mode replaces the type-selector panel with a set of filter controls on the left and a filtered document list on the right.

## Layout

`render_filter_panel` splits the terminal area into a 20/80 horizontal layout (@ref src/tui/views/panels.rs#render_filter_panel). The left panel, titled "Filters", displays three interactive elements stacked vertically: a Status field, a Tag field, and a `[clear filters]` action. The right panel is split 40/60 vertically into a document table and a preview pane. The document table title shows `Documents (n of m)` where `n` is the filtered count and `m` is the total.

## Filter State

Filter state lives on the `App` struct as three fields: `filter_status` (an `Option<Status>`), `filter_tag` (an `Option<String>`), and `filter_focused` (a `FilterField` enum tracking which control has focus) (@ref src/tui/state/app.rs#FilterField). When both filter fields are `None`, the view shows all documents.

The `available_tags` vector is populated when entering Filters mode by `enter_filters_mode`, which collects all unique tags from the store, deduplicates via `BTreeSet`, and sorts alphabetically (@ref src/tui/state/app.rs#enter_filters_mode).

## Field Navigation

`Tab` moves focus to the next field in the cycle Status, Tag, ClearAction and back to Status. `Shift-Tab` reverses the direction (@ref src/tui/state/app.rs#FilterField). The focused field renders in cyan with bold modifier; a field that is not focused but has an active value renders in yellow; otherwise it uses the default style.

## Value Cycling

Pressing `h`/Left or `l`/Right on a focused field cycles through that field's available values (@ref src/tui/views/keys.rs#handle_filters_key).

For status, `cycle_filter_value_next` walks through `None` (displayed as "all"), Draft, Review, Accepted, Rejected, Superseded, then back to `None` (@ref src/tui/state/app.rs#cycle_filter_value_next). `cycle_filter_value_prev` reverses the order (@ref src/tui/state/app.rs#cycle_filter_value_prev).

For tag, cycling forward moves from `None` through the `available_tags` vector in index order, wrapping back to `None` after the last tag. Cycling backward starts from the last tag and works back to `None`.

The ClearAction field ignores value cycling. It responds only to Enter, which calls `reset_filters` (@ref src/tui/state/app.rs#reset_filters).

## Filter Application

The `filtered_docs` method constructs a `Filter` with `doc_type: None`, the current `filter_status`, and the current `filter_tag`, then passes it to `store.list` (@ref src/tui/state/expansion.rs#filtered_docs). Results are sorted by date. The resulting paths are stored in `filtered_docs_cache` to avoid recomputation on every frame.

The cache is invalidated (set to `None`) whenever a filter value changes, when filters are reset, or when certain store-modifying operations occur (status changes, link edits, etc.).

## Document List Navigation

Within the filtered document list, `j`/Down and `k`/Up move the selection cursor. `g` jumps to the first document, `G` to the last. `Ctrl-d` and `Ctrl-u` perform half-page scrolling. Enter on a selected document opens the fullscreen preview. `e` opens the document in the external editor (@ref src/tui/views/keys.rs#handle_filters_key).

## Mode Entry and Exit

The backtick key cycles through view modes. When leaving Filters mode, `cycle_mode` calls `reset_filters` to clear both filter values, reset focus to Status, and invalidate the cache (@ref src/tui/state/app.rs#cycle_mode). When entering Filters mode, `enter_filters_mode` is called and `selected_doc` resets to 0.
