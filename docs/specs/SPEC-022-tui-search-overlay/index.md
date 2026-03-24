---
title: TUI Search Overlay
type: spec
status: draft
author: jkaloger
date: 2026-03-25
tags: [tui, search, overlay]
related:
- related-to: docs/stories/STORY-010-read-only-relations-tab.md
---

## Summary

The search overlay provides a fuzzy-find mechanism for navigating to any document in the store, regardless of which type tab or filter is active. It renders as a full-screen overlay with an input field and a live-updating results list.

## Activation

Pressing `/` in the normal view mode calls @ref src/tui/state/app.rs#enter_search, which sets `search_mode` to true and clears any prior query and results. The `/` binding is also available from the Filters view via @ref src/tui/views/keys.rs#handle_filters_key. Once search mode is active, all key events route through @ref src/tui/views/keys.rs#handle_search_key before any other handler runs.

## Search Index

On startup and after any store mutation (create, delete, reload), @ref src/tui/state/app.rs#rebuild_search_index builds an in-memory index. Each @ref src/tui/state/app.rs#SearchEntry contains the document path and a pre-lowercased string composed of the title, all tags, and the file path, separated by null bytes. This avoids re-lowercasing on every keystroke.

## Query Input

Character keys append to `search_query`. Backspace pops the last character. After each mutation, @ref src/tui/state/app.rs#update_search runs a case-insensitive substring match against the pre-built search index. If the query is empty, results are cleared immediately. Otherwise, matching entries are collected, sorted alphabetically by path, and the selection resets to index zero.

## Result Rendering

@ref src/tui/views/overlays.rs#draw_search_overlay splits the terminal into two vertical regions: a 3-line input field at the top and a results list filling the remainder. The input field displays a cyan `/ ` prompt followed by the current query and a blinking cursor.

Each result row shows three elements: a git status gutter, the document title (left-aligned, 40 characters wide), and the document status with colour coding. The git gutter renders a green `┃` for new files, a yellow `┃` for modified files, and a space otherwise. The currently selected result uses `REVERSED` styling.

## Result Navigation

`Ctrl-j`, `Ctrl-k`, the Down arrow, and the Up arrow move the selection cursor. @ref src/tui/state/app.rs#search_move_down increments the selection index if it is not already at the last result. @ref src/tui/state/app.rs#search_move_up decrements it if it is not already at zero.

## Selection

Pressing Enter calls @ref src/tui/state/app.rs#select_search_result. This looks up the selected path in the store, switches `selected_type` to match the document's type tab, rebuilds the doc tree, positions the cursor on the matching document, and then exits search mode.

## Exit

Pressing Escape calls @ref src/tui/state/app.rs#exit_search, which clears the query, results, and selection, and sets `search_mode` to false. No navigation occurs.
