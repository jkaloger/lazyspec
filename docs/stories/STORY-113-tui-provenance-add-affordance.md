---
title: TUI provenance add affordance
type: story
status: accepted
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-039
priority: should
---




## Context

RFC-039 introduced `provenance` as a free-form frontmatter list. STORY-110
landed the engine field, STORY-111 the `lazyspec provenance add/remove/list`
CLI surface, and STORY-112 the read-only TUI column and detail panel
rendering. STORY-112 explicitly deferred TUI editing.

This story re-opens that deferral for the `add` operation only. Users
viewing a document detail panel in the TUI should be able to append a
provenance entry without dropping to the shell. Removal and other
frontmatter editing remain CLI-only.

The mutation path used by `lazyspec provenance add` already exists in the
engine. This story wires a TUI overlay to that path; it does not introduce
a new engine API.

## Acceptance Criteria

- **Given** the document detail panel is open on a document,
  **when** the user presses the provenance-add keybinding,
  **then** a single-line input overlay appears prompting for a citation.

- **Given** the provenance-add overlay is open,
  **when** the user types a non-empty citation and submits,
  **then** the citation is appended to the document's `provenance`
  frontmatter list and persisted to disk.

- **Given** the provenance-add overlay is open,
  **when** the user submits an empty or whitespace-only input,
  **then** the entry is rejected, no write occurs, and the overlay
  surfaces a validation error.

- **Given** the provenance-add overlay is open,
  **when** the user cancels (Esc),
  **then** the overlay closes and the document frontmatter is unchanged.

- **Given** a successful provenance add,
  **when** the overlay closes,
  **then** the detail panel re-renders showing the new entry, and the
  list-view `Provenance` column for that document reflects the addition.

- **Given** the provenance-add overlay is open,
  **when** the document already contains the same citation string,
  **then** submission is rejected with a duplicate-entry error and no
  write occurs.

- **Given** the user is on a list view (no detail panel focused),
  **when** the user presses the provenance-add keybinding,
  **then** no overlay opens (affordance scoped to detail panel only).

- **Given** an engine write failure during submission,
  **when** the overlay attempts to persist,
  **then** the overlay remains open, surfaces the error, and the in-memory
  store is not updated.

## Scope

### In Scope

- New TUI overlay for entering a single provenance citation.
- Keybinding wired from the detail panel to open the overlay.
- Engine call that appends the entry to frontmatter and persists it,
  reusing the path exercised by `lazyspec provenance add`.
- Store refresh so the new entry appears in the detail panel and list
  view immediately after submission.
- Validation: empty/whitespace rejection, duplicate rejection,
  surface engine errors in the overlay.

### Out of Scope

- Removing entries from the TUI (deferred; CLI remains the path).
- Editing existing entries in place.
- Bulk add or multi-line input.
- Editing other frontmatter fields (author, tags, related) from the TUI.
- A new engine mutation API. Reuse the existing `provenance add` path.
- Search or filter UI keyed on provenance.
- Fullscreen-document-view affordance (detail panel only).
