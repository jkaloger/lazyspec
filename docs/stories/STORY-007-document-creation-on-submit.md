---
title: "Document Creation on Submit"
type: story
status: draft
author: "jkaloger"
date: 2026-03-05
tags: [tui, creation, engine]
related:
  - implements: docs/rfcs/RFC-003-tui-document-creation.md
---

## Context

With the create form UI in place (STORY-006), this story wires the form submission to the engine. When the user presses Enter, the form data is validated, a document is created on disk, and the TUI navigates to show it.

## Acceptance Criteria

### AC1: Submit creates document

**Given** the create form is open with a non-empty title
**When** the user presses Enter
**Then** a new document file is created on disk in the correct directory for the selected type, using the configured naming pattern and template

### AC2: Tags are applied

**Given** the user entered comma-separated tags (e.g. "api, auth, v2")
**When** the form is submitted
**Then** the created document's frontmatter contains those tags as a list

### AC3: Relations are applied

**Given** the user entered a relation string (e.g. "implements:RFC-001")
**When** the form is submitted
**Then** the created document's frontmatter contains a relation of the specified type pointing to the resolved document path

### AC4: Relation shorthand defaults

**Given** the user entered a relation without a type prefix (e.g. "RFC-001")
**When** the form is submitted
**Then** the relation type defaults to `related-to`

### AC5: Title validation

**Given** the create form is open with an empty title
**When** the user presses Enter
**Then** an error message is displayed on the form and no file is created

### AC6: Relation validation

**Given** the user entered a relation referencing a document that doesn't exist (e.g. "RFC-999")
**When** the user presses Enter
**Then** an error message is displayed on the form and no file is created

### AC7: Navigate after creation

**Given** a document was successfully created
**When** the form closes
**Then** the TUI navigates to the newly created document: the type panel selects the correct type, and the document list highlights the new document

### AC8: File watcher pickup

**Given** a document was created via the form
**When** the file watcher detects the new file
**Then** the store is updated and the document appears in the list without requiring manual refresh

## Scope

### In Scope

- Form submission on Enter
- Calling engine create function with form data
- Tag parsing (comma-separated string to list)
- Relation parsing (type prefix + shorthand resolution)
- Validation with inline error display
- Post-creation navigation to new document
- Integration with existing file watcher

### Out of Scope

- Form UI layout and field navigation (covered by STORY-006)
- Editing existing documents
- Deleting documents from TUI
- Bulk creation
