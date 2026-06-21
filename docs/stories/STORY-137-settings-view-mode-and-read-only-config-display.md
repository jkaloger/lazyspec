---
title: Settings view mode and read-only config display
type: story
status: accepted
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

This slice delivers the read-only foundation of the TUI Settings screen (RFC-023): a new `ViewMode::Settings` variant that surfaces the contents of `.lazyspec.toml` inside the existing two-panel TUI shell, with categories on the left and the selected category's settings on the right. The RFC's first goal is discoverability — making the configuration legible from inside the tool — and the RFC explicitly notes that even a purely read-only view is valuable on its own. This slice provides exactly that: entering the view via a number key, navigating across all ten configuration categories, drilling read-only into collection entries (e.g. a single `[[types]]` entry's field list) with an `Esc`-back breadcrumb, and rendering every config field including optional sections shown as `(unset)`. It deliberately stops short of any mutation: no inline editing, no dirty buffer, no write-back, and no reload.

## Acceptance Criteria

### AC1: Number key enters the Settings view

**Given** the TUI is open in any existing view mode (Types, Filters, Graph, etc.)
**When** the user presses the number key bound to Settings
**Then** the active view mode becomes Settings and the screen shows a two-panel layout with a categories list on the left and a settings list on the right

### AC2: All ten categories are listed and navigable

**Given** the Settings view is active and focus is on the categories panel
**When** the user moves the selection down (and back up) through the list with `j`/`k`
**Then** the panel lists exactly the ten categories — General, Document Types, Relationships, Validation Rules, Numbering, GitHub, Coordination, Certification, Agents, and Interface — and the selection can reach the first and last without wrapping past the ends

### AC3: Selecting a category shows its settings on the right

**Given** the Settings view is active
**When** the user selects a category in the left panel
**Then** the right panel updates to display that category's configured fields with their current values read from the loaded config (e.g. selecting General shows `naming.pattern`, `ref_count_ceiling`, and `templates.dir`)

### AC4: Unset optional sections render as `(unset)`

**Given** a loaded config in which an optional section or field is absent (e.g. no `[github]` or `[coordination]` block)
**When** the user selects the category that owns that optional section
**Then** the right panel renders the absent field with the value `(unset)` rather than omitting it or showing a blank

### AC5: Enter drills into a collection entry with a breadcrumb

**Given** the Document Types category is selected and the right panel lists the configured `[[types]]` entries
**When** the user presses `Enter` on a single entry (e.g. `rfc`)
**Then** the right panel replaces the entry list with that entry's field list (name, plural, dir, prefix, icon, numbering, subdirectory, store, singleton, parent_type, agents) and a breadcrumb identifies the drill path (e.g. `Document Types > rfc`)

### AC6: Esc returns from a drilled-in entry to the entry list

**Given** the user has drilled into a collection entry and the breadcrumb is showing
**When** the user presses `Esc`
**Then** the right panel returns to the collection's entry list and the breadcrumb no longer shows the drilled entry

### AC7: Every config field is rendered read-only across all categories

**Given** the Settings view is active with a fully-populated config
**When** the user navigates through every category and drills into each collection (`[[types]]`, `[[relationships]]`, `[[rules]]`, `[certification.overrides]`)
**Then** each configured field defined for that category is displayed with its value, and no keypress modifies, stages, or writes any config value

### AC8: Cycling and quitting from Settings behave consistently

**Given** the Settings view is active
**When** the user presses the mode-cycle key or the quit key
**Then** the mode-cycle key advances to the next view mode and the quit key exits the TUI, without prompting to save (because no edits are possible in this slice)

## Scope

### In Scope

- A new `ViewMode::Settings` variant in `src/tui/state/app.rs`, included in the mode-cycle ordering and `name()` mapping
- Number-key wiring in the normal-key dispatch to enter the Settings view
- A two-panel layout for Settings: categories on the left, settings on the right
- Navigation across all ten categories (General, Document Types, Relationships, Validation Rules, Numbering, GitHub, Coordination, Certification, Agents, Interface)
- Read-only drill-in into collection entries (`[[types]]`, `[[relationships]]`, `[[rules]]`, `[certification.overrides]`) showing the entry's field list, with an `Esc`-back breadcrumb
- Read-only rendering of every config field for each category, including optional sections shown as `(unset)`

### Out of Scope

- Inline editing, the in-memory dirty buffer, and atomic save / save-or-discard prompts (slice 3)
- Making the config reloadable and applying edits live after save, including watching `.lazyspec.toml` (slice 2)
- Dependency auto-scaffolding when a setting requires supporting config (slice 4)
- Collection add and delete mutation for `[[types]]`/`[[relationships]]`/`[[rules]]`/overrides (slice 5)
- The document-impact guard that warns before settings changes affect existing documents (slice 6)
- Statusbar component ordering and hint specifics for the Settings view (slice 7)

