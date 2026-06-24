---
title: Implement graph view pivot sidebar
type: iteration
status: draft
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-153
---<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes

- Depends ITERATION-206 (engine anchor).
- `src/tui/state/app.rs`:
  - `App`: add `graph_anchor: Option<usize>` (index into `doc_types`) near `selected_type` (`:441`).
  - `rebuild_graph()`: pass anchor type name -> `resolve_forest(store, anchor)`.
- `src/tui/views.rs` graph layout (`:1555`): replace empty " Graph " block w/ type-list renderer (reuse/adapt `draw_type_panel` `panels.rs:724`).
- `src/tui/views/keys.rs` `handle_graph_key` (`:662`): `h`/`l` -> move `graph_anchor` prev/next over `doc_types`, then `rebuild_graph`. Mirror `move_type_prev/next` (`:2169`).
- `src/tui/views/keybinds.rs`: register `h`/`l` in graph context.

## Test Plan

- AC1: graph view left col renders type list (not empty block). render/state test.
- AC2: `h`/`l` moves `graph_anchor` over types. key test.
- AC3: anchor set -> `rebuild_graph` calls engine w/ `Some(anchor)`; graph re-roots. state test (assert graph_nodes roots all anchor type).
- AC4: no anchor -> whole-store forest (today). regression.

## Notes

- No re-rooting logic in TUI — engine does it (dictum 3). TUI only selects anchor + renders.
- Reuse types-view sidebar grammar; don't invent new nav.
