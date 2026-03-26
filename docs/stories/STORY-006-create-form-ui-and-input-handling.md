---
title: Create Form UI and Input Handling
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- modal
- input
related:
- implements: RFC-003
---


## Context

The TUI is currently read-only. Users must drop to the CLI to create documents, breaking their browsing flow. This story adds a modal creation form overlay, following the same pattern as the existing search and help overlays.

## Acceptance Criteria

### AC1: Open create form

**Given** the user is in normal mode (not search, fullscreen, or help)
**When** the user presses `n`
**Then** a modal form overlay appears, centered on screen, with the title "Create {Type}" where Type matches the currently selected type in the type panel

### AC2: Form fields

**Given** the create form is open
**When** the form renders
**Then** four labelled fields are displayed: Title, Author, Tags, and Related. Author is pre-filled with a default value. Title has focus.

### AC3: Text input

**Given** the create form is open and a field is focused
**When** the user types characters
**Then** the characters appear in the focused field with a visible cursor

### AC4: Backspace

**Given** a field has text and is focused
**When** the user presses Backspace
**Then** the last character is removed from the field

### AC5: Field navigation

**Given** the create form is open
**When** the user presses Tab
**Then** focus moves to the next field (Title -> Author -> Tags -> Related -> Title). Shift+Tab moves in reverse.

### AC6: Cancel form

**Given** the create form is open
**When** the user presses Esc
**Then** the form closes, all input is discarded, and the user returns to normal mode

### AC7: Visual consistency

**Given** the create form is open
**When** the form renders
**Then** the form uses the same styling conventions as existing overlays (Cyan border for active elements, DarkGray for inactive, centered positioning)

### AC8: Help text

**Given** the create form is open
**When** the form renders
**Then** a footer line shows available actions: Tab (next field), Enter (create), Esc (cancel)

## Scope

### In Scope

- Modal form overlay with four text fields
- Form state management (focused field, field values, open/close)
- Text input and backspace for each field
- Tab/Shift+Tab field navigation
- Esc to cancel
- `n` keybinding to open from normal mode
- Visual rendering consistent with existing overlays

### Out of Scope

- Submitting the form (covered by STORY-007)
- Validation and error display (covered by STORY-007)
- Post-creation navigation (covered by STORY-007)
- Editing existing documents
