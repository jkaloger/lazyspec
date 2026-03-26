---
title: Read-Only Relations Tab
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- implements: RFC-005
---




## Context

The Relations tab previously used a REVERSED highlight on the selected relation, which was visually heavy. The tab should be navigable (j/k to select, Enter to jump) but with a lighter visual treatment that makes it clear focus has shifted away from the document list.

## Acceptance Criteria

### AC1: Subtle selection indicator in relations

- **Given** a document with relations is selected and the Relations tab is active
  **When** the relations list renders
  **Then** the selected relation shows a cyan `>` prefix and bold cyan title, without REVERSED styling

### AC2: j/k navigate relations when Relations tab is active

- **Given** the Relations tab is active
  **When** the user presses `j` or `k`
  **Then** the relation selection changes (not the document list)

### AC3: Enter navigates to selected relation

- **Given** the Relations tab is active and a relation is selected
  **When** the user presses `Enter`
  **Then** the view navigates to the related document and switches back to Preview tab

### AC4: Relations display grouped by type

- **Given** a document with relations is selected
  **When** the user switches to the Relations tab with `Tab`
  **Then** all relations are listed grouped by type, with title and status

### AC5: Document list dims when Relations tab is active

- **Given** the Relations tab is active
  **When** the document list renders
  **Then** all document list items render in dark gray (filenames, status, tags) to indicate focus is elsewhere

## Scope

### In Scope

- Navigable relations with j/k and Enter
- Cyan `>` indicator and bold title for selected relation (no REVERSED)
- Dimmed document list when Relations tab is active
- Relations box gets cyan border when active

### Out of Scope

- Changing the Relations tab grouped layout
- Navigation model changes (STORY-009)
- Types panel border changes (STORY-011)
