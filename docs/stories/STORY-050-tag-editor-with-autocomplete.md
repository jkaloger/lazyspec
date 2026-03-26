---
title: Tag editor with autocomplete
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: RFC-018
---



## Context

Managing tags on documents currently requires manually editing frontmatter or using the CLI. The TUI should support inline tag editing so users can tag documents without leaving the dashboard. This follows the same overlay pattern established by the status picker (STORY-016), applied to tags with autocomplete drawn from the project-wide tag set in `app.store`.

## Acceptance Criteria

### AC1: Open tag editor

- **Given** a document is selected in the document list
  **When** the user presses `t`
  **Then** a tag editor overlay opens showing the document's current tags as removable chips and a text input below them

### AC2: Autocomplete filters project tags

- **Given** the tag editor is open
  **When** the user types into the text input
  **Then** a filtered list of existing project tags appears, excluding tags already on the document, matching by the current input prefix

### AC3: Add existing tag via autocomplete

- **Given** the autocomplete list shows matching tags
  **When** the user selects a tag and presses Enter
  **Then** the tag is added to the document's tag list and the autocomplete list updates to exclude it

### AC4: Add new tag

- **Given** the user has typed a tag name that does not exist in the project
  **When** the user presses Enter
  **Then** a new tag is created and added to the document's tag list

### AC5: Remove last tag with Backspace

- **Given** the tag editor is open and the text input is empty
  **When** the user presses Backspace
  **Then** the last tag in the document's tag list is removed

### AC6: Remove highlighted tag with `d`

- **Given** a tag chip is highlighted in the tag editor
  **When** the user presses `d`
  **Then** that tag is removed from the document's tag list

### AC7: Close and persist

- **Given** the tag editor is open with modified tags
  **When** the user presses Esc
  **Then** the overlay closes, `update_tags()` writes the updated tags to frontmatter, and the store reloads the document via the file watcher

### AC8: Available in Types and Filters modes

- **Given** the TUI is in Types mode or Filters mode
  **When** a document is selected and the user presses `t`
  **Then** the tag editor opens

## Scope

### In Scope

- Tag editor overlay rendering with tag chips and text input
- Autocomplete filtering from `app.store` tag set, excluding current document tags
- Add tag via Enter (existing or new)
- Remove tag via Backspace (last tag) and `d` (highlighted tag)
- Esc to close and write-back via `update_tags()` / `rewrite_frontmatter`
- Works in Types and Filters modes

### Out of Scope

- Table widget changes
- Scroll behavior or scrollbar
- Status picker (STORY-016)
- Batch tag editing across multiple documents
- Tag renaming or deletion from the project-wide tag set
