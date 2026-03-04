---
title: TUI Flat Navigation Model
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
---


## Problem

The current TUI uses a two-panel focus model: `h/l` switches between the Types panel and the DocList panel, then `j/k` navigates within whichever panel has focus. This creates unnecessary friction. Users must mentally track which panel is active, and the double-border/cyan highlight on the active panel is the only indicator of where their cursor lives. The Types panel gets its own focus state despite containing only four items that rarely change.

The relations tab also participates in this complexity. When the Relations tab is active and DocList is focused, `j/k` hijacks navigation to move through relations instead of documents. This context-dependent rebinding is surprising.

## Intent

Replace the two-panel focus model with a flat navigation model:

- `h/l` cycles through document types (the four items in the Types panel)
- `j/k` always moves through documents of the currently selected type
- The Relations tab becomes read-only with no selection highlight and no keyboard navigation

This eliminates the `Panel` enum and the concept of "active panel" entirely. The Types panel becomes a passive indicator showing which type is selected, and the document list is always the navigable surface.

## Current Behaviour

```
h/l  → switch active_panel between Panel::Types and Panel::DocList
j/k  → navigate within whichever panel is focused
       (or navigate relations when Relations tab is active + DocList focused)
Tab  → toggle between Preview and Relations tabs
Enter → fullscreen (Preview tab) or navigate to relation (Relations tab)
```

Border highlighting: active panel gets cyan double-border, inactive gets dark gray plain border.

## Proposed Behaviour

```
h/l  → cycle selected_type through doc_types (clamping at boundaries)
j/k  → navigate documents (Preview tab) or relations (Relations tab)
Tab  → toggle between Preview and Relations tabs (unchanged)
Enter → fullscreen document (Preview tab) or navigate to relation (Relations tab)
```

Border highlighting follows focus. The Types panel always uses a static plain/gray border. The doc list gets cyan/double when it has focus (Preview tab) and dims to plain/gray when focus shifts to relations. The relations panel gets a cyan border when active. The selected relation uses a cyan `>` indicator and bold title rather than REVERSED styling. Doc list items dim to dark gray when Relations tab is active.

## Interface Sketch

### State changes

Remove `active_panel: Panel` and `selected_relation: usize` from `App`.

Remove the `Panel` enum.

```
@ref src/tui/app.rs#App

// Remove: active_panel, selected_relation
// Remove: Panel enum
// Remove: move_relation_up, move_relation_down, navigate_to_relation
```

`move_down` / `move_up` no longer match on `active_panel`. They always operate on `selected_doc`.

Add `move_type_next` and `move_type_prev` methods that cycle `selected_type` and reset `selected_doc` to 0.

### Key handling changes

```
@ref src/tui/mod.rs

// h/l → app.move_type_prev() / app.move_type_next()
// j/k → app.move_down() / app.move_up() (always doc list)
// Enter → app.enter_fullscreen() (always, remove relation navigation branch)
// Remove relation j/k hijack
```

### Rendering changes

```
@ref src/tui/ui.rs#draw_type_panel

// Remove active_panel border logic
// Types panel: always plain border, dark gray
// Selected type: cyan + bold (unchanged)
```

```
@ref src/tui/ui.rs#draw_doc_list

// Always cyan double-border (no conditional)
// Selected doc: REVERSED (unchanged)
```

```
@ref src/tui/ui.rs#draw_relations_content

// Remove selected_relation indicator ("> ")
// Remove REVERSED style on selected relation
// All items rendered with uniform style
```

```
@ref src/tui/ui.rs#draw_help_overlay

// Update "h/l Switch panels" → "h/l Switch type"
```

### Delete guard

Currently `d` only works when `active_panel == Panel::DocList`. With the panel concept removed, `d` should always work (it operates on the selected document). Guard on `selected_doc_meta().is_some()` instead.

## Stories

1. **Flatten navigation model** -- Remove `Panel` enum, change `h/l` to cycle types, `j/k` to always navigate docs. Update key handling and movement methods.
2. **Make relations tab read-only** -- Remove `selected_relation` state, strip selection indicators from relations rendering, remove relation navigation from key handler.
3. **Update border highlighting** -- Remove conditional border styles from type panel and doc list. Types panel gets static plain border, doc list always gets active border.
