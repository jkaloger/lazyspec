---
title: Consistent settings rendering and interaction
type: story
status: accepted
author: jkaloger
date: 2026-06-20
tags: []
related:
- implements: RFC-023
---

## Context

Stories 1-7 shipped the settings screen. Two inconsistencies remain against the rest of the TUI.

Rendering: every other data view (`draw_doc_list`, `draw_agents_screen`, relations preview) uses a ratatui `Table` with a header row, column widths, and `TableState` selection. The settings right panel is the only one rendered as a `Paragraph` of `"label: value"` lines, with cursor highlight, edit-errors, and scaffold prompts spliced into the line vec by hand.

Interaction: field interaction is split across two keys. `Enter` starts an inline edit but only for text-family editors; bool and enum fields are mutated with `Space` instead, and `Enter` on them does nothing. The user must know which key a given field type wants.

This story aligns the right panel with the table rendering and makes `Enter` the single entry point for field interaction, dropping `Space`. Enum selection moves from a hidden `Space`-cycle to a variant-picker overlay (RFC-023 interaction-model amendment).

## Acceptance Criteria

- **Given** the settings screen with a non-collection category selected
  **When** the right panel renders
  **Then** it is a two-column table (field, value) with a header row, rendered with the same `Table` widget family as `draw_doc_list`, and the focused field is marked by the table's selection cursor rather than restyled text.

- **Given** a focused text-family field (text, bounded-numeric, nullable, duration, or list)
  **When** the user presses `Enter`
  **Then** an inline edit begins on that field; `Enter` confirms the value into the buffer, `Esc` cancels, matching the pre-existing inline-edit behaviour.

- **Given** a focused bool field
  **When** the user presses `Enter`
  **Then** the value flips in the buffer and the buffer is marked dirty.

- **Given** a focused enum field (numbering, store, reserved format, rule severity, or rule shape)
  **When** the user presses `Enter`
  **Then** a variant-picker overlay opens listing that enum's variants; `j`/`k` move the selection, `Enter` writes the chosen variant into the buffer, and `Esc` closes it without change.

- **Given** the variant picker selects `sqids`/`reserved` numbering or `github-issues` store on a type
  **When** the selection is written
  **Then** the existing dependency auto-scaffolding fires exactly as it did under the old enum-cycle path.

- **Given** a focused status-bar zone field
  **When** the user presses `Enter`
  **Then** the zone-ordering editor opens, as before.

- **Given** a focused read-only field
  **When** the user presses `Enter`
  **Then** nothing happens.

- **Given** any settings field
  **When** the user presses `Space`
  **Then** nothing happens; `Space` is no longer bound in the settings view.

- **Given** a collection category showing its entry list
  **When** the user presses `Enter` on an entry
  **Then** it still drills into that entry's field table, unchanged.

- **Given** an inline edit that fails field-level validation, or a pending dependency-scaffold prompt
  **When** the right panel renders
  **Then** the error/prompt is shown alongside the table (not lost), with the same information as before the table migration.

## Scope

### In Scope

- Re-rendering the settings right panel (field-view and entry-list) as a `Table`.
- Field cursor via `TableState` selection.
- `Enter` as the single field-interaction trigger, dispatched by editor type.
- A variant-picker overlay for enum fields, covering all enum editors.
- Removing the `Space` binding in the settings view.
- Migrating inline edit-error and dependency-scaffold prompt rendering to coexist with the table.
- Updating help/keybinding hints that reference `Space` for settings.

### Out of Scope

- Any change to the dirty-buffer, atomic-save, or validation logic (Stories 3-6).
- The status-bar zone-ordering editor's internals (Story 7); only its `Enter` entry point is in scope.
- Left-panel Categories rendering (already a `List`).
- New config fields or categories.
- Moving/renaming/renumbering documents on disk.
