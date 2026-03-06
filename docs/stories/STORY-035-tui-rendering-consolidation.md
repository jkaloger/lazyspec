---
title: TUI Rendering Consolidation
type: story
status: draft
author: jkaloger
date: 2026-03-06
tags:
- refactor
- tui
related:
- implements: docs/rfcs/RFC-012-architecture-review-yagni-dry-cleanup.md
---


## Context

The TUI's document list rendering logic is duplicated between `draw_doc_list`
(normal mode) and `draw_filters_mode`. Both build `ListItem` spans with the
same status colouring, title truncation, and tag display with overflow
counting. The tag-colour-span loop also appears three times in `ui.rs`.

## Acceptance Criteria

- **Given** the TUI renders a document list in normal mode
  **When** the list item construction logic is examined
  **Then** it uses a shared helper function, not inline span building

- **Given** the TUI renders a document list in filters mode
  **When** the list item construction logic is examined
  **Then** it uses the same shared helper as normal mode

- **Given** tags are rendered anywhere in the TUI
  **When** the tag-to-styled-span conversion is examined
  **Then** it uses a single shared function

- **Given** any TUI view that displays documents
  **When** this refactor is complete
  **Then** the visual output is identical to before

## Scope

### In Scope

- Extracting document list-item builder into a shared helper
- Extracting tag-span rendering into a shared function
- Both used by `draw_doc_list` and `draw_filters_mode`

### Out of Scope

- Changing visual styling or layout
- Adding new TUI features or modes
- Refactoring non-list rendering code
