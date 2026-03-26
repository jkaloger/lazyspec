---
title: Direct Mode Switching
type: story
status: draft
author: jkaloger
date: 2026-03-06
tags:
- tui
- ux
related:
- implements: RFC-011
---



## Context

Mode switching currently uses backtick, which cycles sequentially through four modes. This has two problems: backtick is obscure (most users wouldn't guess it), and cycling means up to three keypresses to reach the desired mode. The title bar shows `` [Types] ` to cycle `` which reveals the mechanism but not the destination -- users can't see which modes exist or where they are in the cycle.

## Acceptance Criteria

### AC1: Number keys switch modes directly

- **Given** the TUI is in any mode and no modal is active
  **When** the user presses `1`, `2`, `3`, or `4`
  **Then** the mode switches to Types, Filters, Metrics, or Graph respectively

### AC2: Mode strip in title bar

- **Given** the TUI is running
  **When** the screen renders
  **Then** the title bar shows all modes with their number keys: `[1]Types [2]Filters [3]Metrics [4]Graph`

### AC3: Active mode is highlighted

- **Given** the TUI is in a specific mode
  **When** the screen renders
  **Then** the active mode in the title bar is rendered in cyan/bold and inactive modes in DarkGray

### AC4: Backtick cycling removed

- **Given** the TUI is running
  **When** the user presses backtick
  **Then** nothing happens (the key is unbound)

### AC5: Number keys ignored during modals

- **Given** a modal is active (create form, delete confirm, search, help overlay)
  **When** the user presses `1`, `2`, `3`, or `4`
  **Then** the mode does not change (the key is handled by the modal or ignored)

## Scope

### In Scope

- Number key handlers (`1-4`) in the normal key dispatch
- Title bar mode strip rendering with active highlighting
- Removing backtick key binding and `cycle_mode` usage
- Adding `set_mode(ViewMode)` method

### Out of Scope

- Changing the ViewMode enum itself
- Adding or removing modes
- Mode-specific state reset logic (already handled by existing code)
