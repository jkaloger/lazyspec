---
title: Scroll padding and half-page navigation
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: docs/rfcs/RFC-018-tui-interaction-enhancements.md
---



## Context

The document list viewport currently snaps to keep the selection visible, which creates a jarring scroll experience. The selection can sit at the very edge of the visible area, giving no context about surrounding items. There are also no keybindings for jumping larger distances, forcing users to hold `j`/`k` to traverse long lists.

This story adds vim-style `scrolloff=2` padding and sticky viewport behaviour, along with `Ctrl-D`/`Ctrl-U` half-page jumps. These apply to document lists (Types, Filters modes) and fullscreen preview.

## Acceptance Criteria

### AC1: Scroll padding when moving down

- **Given** a document list with more items than the visible height
  **When** the user presses `j` and the selection is within 2 rows of the bottom edge
  **Then** the viewport scrolls down to maintain at least 2 items visible below the selection

### AC2: Scroll padding when moving up

- **Given** the viewport is scrolled partway through a document list
  **When** the user presses `k` and the selection is within 2 rows of the top edge
  **Then** the viewport scrolls up to maintain at least 2 items visible above the selection

### AC3: Sticky viewport on scroll-up

- **Given** the user has scrolled down past the initial viewport
  **When** the user presses `k` to move the selection up
  **Then** the viewport stays in place until the selection reaches the top padding boundary (2 rows from the top edge)

### AC4: Padding clamped at list boundaries

- **Given** the selection is near the first or last item in the list
  **When** fewer than 2 items exist above or below the selection
  **Then** the padding is reduced gracefully (no blank rows, no out-of-bounds scroll)

### AC5: Ctrl-D half-page down in document lists

- **Given** the user is in Types or Filters mode
  **When** the user presses `Ctrl-D`
  **Then** the selection moves down by `visible_height / 2` rows and the viewport adjusts to maintain scroll padding

### AC6: Ctrl-U half-page up in document lists

- **Given** the user is in Types or Filters mode
  **When** the user presses `Ctrl-U`
  **Then** the selection moves up by `visible_height / 2` rows and the viewport adjusts to maintain scroll padding

### AC7: Half-page jump clamped at boundaries

- **Given** the selection is fewer than `visible_height / 2` rows from the start or end of the list
  **When** the user presses `Ctrl-U` or `Ctrl-D`
  **Then** the selection clamps to the first or last item respectively

### AC8: Ctrl-D / Ctrl-U in fullscreen preview

- **Given** the user is in fullscreen preview mode
  **When** the user presses `Ctrl-D` or `Ctrl-U`
  **Then** the `scroll_offset` adjusts by `visible_height / 2` (no selection cursor, just viewport movement)

### AC9: Half-page keys inactive in modal states

- **Given** a modal is open (create form, delete confirm, search overlay)
  **When** the user presses `Ctrl-D` or `Ctrl-U`
  **Then** the keypress is ignored

## Scope

### In Scope

- Manual viewport offset management replacing `ListState` auto-scroll
- `scrolloff=2` padding above and below the selection
- Sticky viewport on scroll-up (viewport holds until selection hits top padding)
- `Ctrl-D` / `Ctrl-U` half-page jumps in Types and Filters modes
- `Ctrl-D` / `Ctrl-U` for fullscreen preview scroll offset
- Boundary clamping for both single-step and half-page movement
- Key dispatch changes in `handle_key_event` and `handle_fullscreen_key`

### Out of Scope

- Table widget changes (STORY-047)
- Scrollbar rendering
- Tag editing
- Changes to modal input handling
