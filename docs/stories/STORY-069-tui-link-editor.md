---
title: TUI Link Editor
type: story
status: draft
author: jkaloger
date: 2026-03-20
tags: []
related:
- implements: RFC-028
---



## Context

Adding and removing document relationships currently requires the CLI or manual frontmatter editing. The TUI should support inline link management so users can wire up relationships without leaving the dashboard. This follows the overlay pattern established by the tag editor (STORY-050) and status picker (STORY-016), applied to the Relations panel. The link editor uses fuzzy search over all documents and cycles through the four relationship types defined in RFC-028.

## Acceptance Criteria

### AC1: Open link editor overlay

- **Given** the Relations panel is focused and a document is selected
  **When** the user presses `r`
  **Then** a link editor overlay opens with two fields: a relationship type selector and a target document search input

### AC2: Fuzzy search filters documents in real time

- **Given** the link editor overlay is open
  **When** the user types into the target document search field
  **Then** the document list filters in real time, matching against both shorthand ID and title

### AC3: Documents displayed as TYPE-NNN: Title

- **Given** the link editor overlay is open and search results are visible
  **When** the user views the filtered document list
  **Then** each result is displayed in the format `TYPE-NNN: Title` (e.g. `RFC-028: Document Reference Ergonomics`)

### AC4: Relationship type cycling with Tab

- **Given** the link editor overlay is open
  **When** the user presses `Tab`
  **Then** the relationship type field cycles through: `implements`, `supersedes`, `blocks`, `related-to`

### AC5: Add link with Enter

- **Given** a target document is selected and a relationship type is chosen
  **When** the user presses `Enter`
  **Then** the link is added to the source document's frontmatter `related` field and the overlay closes

### AC6: Cancel with Esc

- **Given** the link editor overlay is open
  **When** the user presses `Esc`
  **Then** the overlay closes without making any changes to the document

### AC7: Delete existing relationship with confirmation

- **Given** the Relations panel is focused and an existing relationship is selected
  **When** the user presses `d`
  **Then** a confirmation prompt appears asking the user to confirm deletion

### AC8: Confirmed deletion removes link from frontmatter

- **Given** the deletion confirmation prompt is showing
  **When** the user confirms (presses `y`)
  **Then** the relationship is removed from the document's frontmatter `related` field and the Relations panel updates

### AC9: Overlay requires a selected document

- **Given** no document is selected in the document list
  **When** the user presses `r`
  **Then** the link editor overlay does not open

### AC10: Self-linking is prevented

- **Given** the link editor overlay is open
  **When** the user searches for documents
  **Then** the currently selected document is excluded from the search results, preventing self-referential links

## Scope

### In Scope

- `r` key binding to open link editor overlay from Relations panel
- Overlay with relationship type selector and fuzzy search input
- Fuzzy search over all documents, displayed as `TYPE-NNN: Title`
- `Tab` to cycle relationship type through `implements`, `supersedes`, `blocks`, `related-to`
- `Enter` to confirm and write link to frontmatter via store operations
- `Esc` to cancel without changes
- `d` on existing relationship in Relations panel to delete with `y`/`n` confirmation
- Self-link prevention by excluding the current document from search results

### Out of Scope

- CLI `link` / `unlink` command changes (STORY-068)
- Shell completions (STORY-070)
- Changes to how the Relations panel displays existing relationships
- Changes to the document creation form
- Batch link editing across multiple documents
