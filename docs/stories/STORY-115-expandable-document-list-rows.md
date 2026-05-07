---
title: Expandable document list rows
type: story
status: accepted
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-040
---



## Context

Implements RFC-040 section 1: `x` toggles a TUI-wide wrap mode; when on, the currently selected document list row wraps its content (title, tags, provenance) to multiple lines up to a configurable max height. All other rows stay at 1 line. Moving the selection moves which row wraps. (`e` reserved for opening the external editor.)

## Acceptance Criteria

- **AC1:** **Given** wrap mode is off, **When** I press `x`, **Then** wrap mode turns on and the selected row wraps its content up to max_expanded_height
- **AC2:** **Given** wrap mode is on, **When** I press `x` again, **Then** wrap mode turns off and all rows render on 1 line
- **AC3:** **Given** wrap mode is on, **When** I move selection to a different row (e.g. `j`/`k`), **Then** the previously selected row collapses to 1 line and the newly selected row wraps
- **AC4:** **Given** the config file with max_expanded_height set, **When** the selected row wraps, **Then** it expands to at most that height
- **AC5:** **Given** the selected row has many tags, **When** wrap mode is on, **Then** all tags render across multiple lines without truncation or `+N` counter
- **AC6:** **Given** the selected row has a long title, **When** wrap mode is on, **Then** the title wraps at word boundaries within the title cell width

## Scope

### In Scope

- Default row height of 1 line in document list
- `x` keybinding to toggle global `wrap_mode` boolean (`e` reserved for editor)
- Selected row renders title/tags/provenance wrapped when `wrap_mode` is on
- Configuration: max_expanded_height (default 5)
- Title and provenance wrapped via `textwrap`; tags packed into styled `[name]` spans across lines without splitting a tag

### Out of Scope

- Wrap in views other than document list
- Mouse interaction for toggling wrap mode
- Persisting wrap mode between sessions
- Per-row wrap toggling (wrap mode is global)
- Visual indicator column (removed)
- Horizontal expansion of rows
