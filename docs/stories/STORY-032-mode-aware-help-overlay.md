---
title: Mode-Aware Help Overlay
type: story
status: draft
author: jkaloger
date: 2026-03-06
tags:
- tui
- ux
related:
- implements: docs/rfcs/RFC-011-tui-ux-refinements.md
---


## Context

The TUI has a help overlay triggered by `?`, but nothing on screen tells users it exists. New users have no way to discover keybindings without prior knowledge. The overlay is also static -- it shows the same keybindings regardless of which mode is active, including keys that don't apply in the current context.

## Acceptance Criteria

### AC1: Persistent help hint

- **Given** the TUI is running in any mode
  **When** the screen renders
  **Then** a dim `? help` label appears in the bottom-left corner

### AC2: Help hint does not interfere with content

- **Given** the TUI is running with a small terminal size
  **When** the screen renders
  **Then** the help hint does not overlap or push other content out of view

### AC3: Mode-specific keybindings in overlay

- **Given** the user is in Types mode and presses `?`
  **When** the help overlay renders
  **Then** it shows keybindings relevant to Types mode (navigation, actions, mode switching)

### AC4: Graph mode includes visual legend

- **Given** the user is in Graph mode and presses `?`
  **When** the help overlay renders
  **Then** it includes the icon legend (RFC, ADR, Story, Iteration symbols) and edge type legend in addition to Graph-specific keybindings

### AC5: Help overlay reflects current mode name

- **Given** the user is in any mode and presses `?`
  **When** the help overlay renders
  **Then** the overlay title or header includes the current mode name

### AC6: Dismiss behaviour unchanged

- **Given** the help overlay is open
  **When** the user presses `?` or `Esc`
  **Then** the overlay closes

## Scope

### In Scope

- Persistent `? help` label in bottom-left corner (DarkGray styling)
- Refactoring `draw_help_overlay` to accept `&App` for mode awareness
- Mode-specific keybinding sections for each ViewMode
- Graph icon and edge legend within the Graph mode help
- Mode name in overlay header

### Out of Scope

- Interactive help (clickable keybindings, tutorials)
- Context-sensitive help beyond mode level (e.g. different help during search)
- Customisable keybindings
