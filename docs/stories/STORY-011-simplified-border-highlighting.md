---
title: Simplified Border Highlighting
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- implements: RFC-005
---




## Context

With the panel focus model removed, border styling shifts from tracking which panel is active to tracking which surface the user is interacting with. The Types panel is never navigable, so it always gets a passive border. The doc list and relations panel trade focus via the Tab key, and their borders reflect this.

## Acceptance Criteria

### AC1: Types panel has a static border

- **Given** the dashboard is displayed
  **When** the Types panel renders
  **Then** it always uses a plain border with dark gray colour, regardless of any selection state

### AC2: Document list has an active border by default

- **Given** the Preview tab is active
  **When** the document list renders
  **Then** it uses a double border with cyan colour

### AC3: Document list border dims when Relations tab is active

- **Given** the Relations tab is active
  **When** the document list renders
  **Then** it uses a plain border with dark gray colour

### AC4: Relations panel gets an active border when focused

- **Given** the Relations tab is active
  **When** the relations panel renders
  **Then** it uses a cyan border

### AC5: Help overlay reflects new keybindings

- **Given** the help overlay is open
  **When** the user reads the keybinding list
  **Then** `h/l` is described as "Switch type" (not "Switch panels")

## Scope

### In Scope

- Types panel: always plain border, dark gray
- Doc list: cyan/double when Preview tab active, plain/gray when Relations tab active
- Relations panel: cyan border when active, gray when inactive
- Updating help overlay text

### Out of Scope

- Layout changes (panel sizes, positions)
- Navigation model changes (STORY-009)
- Relation navigation styling (STORY-010)
