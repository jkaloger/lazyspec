---
title: Settings Interface category and status bar ordering
type: story
status: draft
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

This slice adds the **Interface** settings category to the TUI settings screen, mapping the `[tui]` config block to the inline scalar and list editors delivered in slice 3. The category exposes the full `UiConfig` surface: `ascii_diagrams` (boolean), `[tui.multiline].max_expanded_height` (bounded numeric, default 5), and the `[tui.statusbar]` group — `enabled` (boolean, default true) plus the three component-ordering lists `left`, `center`, and `right`, each an optional ordered list of status-bar component names. The distinctive control is the ordering editor, which lets the user choose which status-bar components appear in each zone and in what order, composing the component vocabulary defined by RFC-022 (TUI Status Bar). Because that vocabulary and the rendering it drives are owned by RFC-022, which is accepted but not yet implemented, this story is authorable now but its BUILD is gated on RFC-022 landing first.

## Acceptance Criteria

### AC1: Interface category surfaces the full UiConfig

**Given** the settings screen is open on the Interface category
**When** the category renders its editable fields
**Then** `ascii_diagrams`, `multiline.max_expanded_height`, `statusbar.enabled`, and the `statusbar.left`/`center`/`right` ordering lists are each shown as an editable row reflecting the current `[tui]` config values (or their defaults when unset).

### AC2: Boolean fields edit through the inline boolean editor

**Given** the Interface category is focused on `ascii_diagrams` (or `statusbar.enabled`)
**When** the user toggles the field with the slice-3 inline boolean editor and commits the save
**Then** the corresponding `UiConfig` value is written and the persisted `[tui]` config reflects the new boolean.

### AC3: max_expanded_height is bounded numeric editing

**Given** the Interface category is focused on `multiline.max_expanded_height`
**When** the user enters a value through the inline numeric editor
**Then** a valid positive integer is accepted and saved, an out-of-bounds or non-numeric entry is rejected with the field retaining its prior value, and an unset field shows the default of 5.

### AC4: Status-bar zone ordering edits the component lists

**Given** the Interface category is focused on a status-bar zone (`left`, `center`, or `right`)
**When** the user uses the ordering editor to add, remove, or reorder components for that zone and commits the save
**Then** the zone's `Option<Vec<String>>` is written in the user's chosen order, and a zone left untouched is saved as before (an explicitly cleared zone persisting as an empty/absent list per the slice-3 list semantics).

### AC5: Ordering editor offers RFC-022's component vocabulary

**Given** the ordering editor is open for a status-bar zone
**When** the user is choosing which components to place in the zone
**Then** the selectable component names are exactly those defined by RFC-022's status-bar component vocabulary, and a name not in that vocabulary is not offered.

### AC6: BUILD is gated on RFC-022 implementation

**Given** RFC-022 (TUI Status Bar) is accepted but not yet implemented
**When** this story is scheduled for BUILD
**Then** implementation does not start until RFC-022's status bar (its component vocabulary and rendering) is implemented, because the ordering editor's component set and the resulting config are only meaningful against that vocabulary.

## Scope

### In Scope
- The Interface settings category mapping the `[tui]` config block to editable rows.
- Wiring `ascii_diagrams` and `statusbar.enabled` to the slice-3 inline boolean editor.
- Wiring `multiline.max_expanded_height` to the slice-3 bounded numeric editor, defaulting to 5.
- The status-bar zone-ordering editor for `statusbar.left`/`center`/`right`, composing RFC-022's component vocabulary into each zone in user-chosen order.
- Persisting Interface-category edits through the slice-3 atomic save path.

### Out of Scope
- The base inline scalar and list editors and the atomic save mechanism — owned by slice 3; this slice only configures the Interface category on top of them.
- Status-bar rendering, the component implementations, and the component vocabulary itself — owned by RFC-022 (TUI Status Bar).
- The read-only settings view — owned by slice 1.
- Settings categories other than Interface (e.g. document types, relationships, agents) — owned by their respective slices.

