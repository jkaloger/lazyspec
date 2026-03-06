---
title: Subfolder Document Discovery
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags: []
related:
- implements: docs/rfcs/RFC-010-subfolder-document-support.md
---



## Context

Authors want to co-locate supporting assets (images, diagrams, notes) alongside their documents. Currently lazyspec only discovers flat `.md` files in each doc directory, so there's no natural place to put these assets. This story adds support for folder-based documents where a subfolder contains an `index.md` entrypoint.

## Acceptance Criteria

- **Given** a doc directory contains a subfolder with an `index.md` file
  **When** lazyspec loads the project
  **Then** the `index.md` is discovered and parsed as a document, with its path set to the full relative path (e.g. `docs/rfcs/RFC-010-something/index.md`)

- **Given** a doc directory contains both flat `.md` files and subfolders with `index.md`
  **When** lazyspec loads the project
  **Then** both flat files and folder-based documents are discovered

- **Given** a subfolder exists in a doc directory but does not contain `index.md`
  **When** lazyspec loads the project
  **Then** the subfolder is silently ignored

- **Given** a folder-based document exists
  **When** a user references it by shorthand (e.g. `RFC-010`)
  **Then** shorthand resolution matches against the folder name and resolves to the `index.md` document

- **Given** a folder-based document exists
  **When** another document links to it via the full path in a relationship
  **Then** the relationship resolves correctly in validation, status, and show commands

- **Given** a folder-based document exists
  **When** a user runs `lazyspec search` with a matching term
  **Then** the folder-based document appears in search results

## Scope

### In Scope

- Extending document discovery to find `index.md` inside subfolders
- Shorthand resolution against folder names
- Relationship resolution using the full `folder/index.md` path
- All existing commands (show, status, validate, search, list) working with folder-based documents

### Out of Scope

- Awareness of non-`index.md` files inside the folder
- The `lazyspec create` command producing folder-based documents (separate story)
- Recursive discovery beyond one level of nesting
- Relationships between `index.md` and sibling files
