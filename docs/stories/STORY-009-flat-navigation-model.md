---
title: Flat Navigation Model
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- implements: RFC-005
---




## Context

The TUI currently uses a two-panel focus model where `h/l` switches between the Types and DocList panels. This means users must track which panel is active before `j/k` does what they expect. Flattening to a single navigation axis removes this cognitive overhead: `h/l` picks the type, `j/k` picks the document.

## Acceptance Criteria

### AC1: h moves to the previous document type

- **Given** the selected type is Story
  **When** the user presses `h`
  **Then** the selected type becomes Adr (the previous type in the list)
  and the selected document resets to the first document of that type

### AC2: l moves to the next document type

- **Given** the selected type is Rfc
  **When** the user presses `l`
  **Then** the selected type becomes Adr (the next type in the list)
  and the selected document resets to the first document of that type

### AC3: h at the first type does not wrap

- **Given** the selected type is Rfc (first in list)
  **When** the user presses `h`
  **Then** the selected type remains Rfc

### AC4: l at the last type does not wrap

- **Given** the selected type is Iteration (last in list)
  **When** the user presses `l`
  **Then** the selected type remains Iteration

### AC5: j/k always navigate documents

- **Given** any document type is selected with multiple documents
  **When** the user presses `j`
  **Then** the next document in the list is selected
  regardless of any previously active panel state

### AC6: Enter opens fullscreen

- **Given** a document is selected
  **When** the user presses `Enter`
  **Then** the document opens in fullscreen view
  regardless of which preview tab is active

### AC7: d deletes the selected document

- **Given** a document is selected
  **When** the user presses `d`
  **Then** the delete confirmation dialog opens for that document

### AC8: d with no documents is a no-op

- **Given** the current type has no documents
  **When** the user presses `d`
  **Then** nothing happens

## Scope

### In Scope

- Changing `h/l` to cycle through document types
- Making `j/k` always operate on the document list
- Changing `Enter` to always open fullscreen
- Removing the `Panel` enum and `active_panel` state
- Updating the `d` key guard

### Out of Scope

- Border highlighting changes (STORY-011)
- Relations tab navigation changes (STORY-010)
- Changes to search, create form, or fullscreen key handling
