---
title: TUI warnings panel
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: RFC-015
---




## Context

Parse errors collected by the Store (STORY-044) need to be visible in the TUI so users can discover broken documents without switching to the CLI. This story adds a toggleable warnings panel.

## Acceptance Criteria

### AC1: Warnings panel toggles with `w`

- **Given** the TUI is running and the store has parse errors
  **When** the user presses `w`
  **Then** a warnings panel appears showing parse failure entries

### AC2: Panel shows path and error

- **Given** the warnings panel is open
  **When** the user views an entry
  **Then** each entry displays the file path and the parse error message

### AC3: Panel is scrollable

- **Given** the warnings panel is open and there are more errors than fit on screen
  **When** the user scrolls (j/k or arrow keys)
  **Then** the list scrolls to reveal additional entries

### AC4: Panel dismisses with `w` or Escape

- **Given** the warnings panel is open
  **When** the user presses `w` or `Escape`
  **Then** the panel closes and the main view is restored

### AC5: No panel when no errors

- **Given** the store has no parse errors
  **When** the user presses `w`
  **Then** nothing happens (no empty panel is shown)

## Scope

### In Scope

- Toggleable overlay/panel triggered by `w`
- Scrollable list of parse errors
- Dismiss with `w` or Escape

### Out of Scope

- Fixing documents from within the TUI
- Inline indicators on the document list
- Status bar error count (could be a follow-up)
