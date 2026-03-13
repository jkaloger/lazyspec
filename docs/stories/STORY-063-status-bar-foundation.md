---
title: Status bar foundation
type: story
status: accepted
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: docs/rfcs/RFC-021-lualine-inspired-status-bar.md
---



## Context

The lazyspec TUI lacks a persistent status bar. The header shows a title and a
mode hint, but there's no unified footer across screens. RFC-021 defines a
lualine-inspired 3-section status bar. This story delivers the infrastructure
and two basic components.

## Acceptance Criteria

- **Given** the TUI is open on any ViewMode (Types, Filters, Metrics, Graph, Agents)
  **When** the screen renders
  **Then** a single-line footer is visible at the bottom with left, center, and right sections

- **Given** the user is on any screen
  **When** they look at the status bar's left section
  **Then** the current panel/mode name is displayed (e.g. "Types", "Filters", "Agents")

- **Given** the user is on any screen
  **When** they look at the status bar's right section
  **Then** a help hint ("? for help") is displayed

- **Given** the header previously showed `[Types] backtick to cycle`
  **When** the status bar is active
  **Then** the mode indicator is removed from the header (the header shows only "lazyspec")

## Scope

### In Scope

- Footer layout added to the main `draw()` function for all ViewModes
- 3-section rendering (left-aligned, centered, right-aligned) within the footer
- Panel indicator component
- Help hint component
- Header cleanup (remove mode indicator)

### Out of Scope

- Git branch component (STORY-064)
- Parent/child breadcrumb (STORY-064)
- Agents footer migration (STORY-064)
- Powerline separators or mode-aware color theming
