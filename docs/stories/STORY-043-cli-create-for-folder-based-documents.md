---
title: CLI create for folder-based documents
type: story
status: draft
author: jkaloger
date: 2026-03-07
tags: []
related:
- implements: RFC-014
---



## Context

With folder-based documents and child discovery in place, authors need a way to create these structures through the CLI rather than manually creating folders and files. This story is intentionally left with open design questions for future iteration.

## Acceptance Criteria

### AC1: Create a folder-based document

- **Given** the user wants a new folder-based document
  **When** the user runs `lazyspec create` with a folder flag
  **Then** a directory is created with the standard naming convention and an `index.md` containing the frontmatter template

### AC2: Add a child to an existing folder-based document

- **Given** a folder-based document already exists
  **When** the user runs a create command targeting that folder
  **Then** a new `.md` file is created inside the folder with frontmatter template

## Open Issues

> [!WARNING]
> These questions need answers before this story moves to iteration.

- How should the CLI handle converting a flat `.md` file into a folder-based document? Should it move the flat file to `folder/index.md` automatically, or refuse and ask the user to restructure manually?
- What naming convention should child files follow? Free-form, or a prefix/numbering scheme?
- Should `create` infer the child's type from the parent folder prefix, or require it explicitly?

## Scope

### In Scope

- `lazyspec create` producing folder-based documents
- `lazyspec create` adding child documents to existing folders
- Frontmatter template generation for children

### Out of Scope

- Engine discovery (STORY-040)
- CLI output formatting (STORY-041)
- TUI rendering (STORY-042)
- Automatic flat-to-folder migration (open issue, may become its own story)
