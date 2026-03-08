---
title: Child document discovery
type: story
status: accepted
author: jkaloger
date: 2026-03-07
tags: []
related:
- implements: docs/rfcs/RFC-014-nested-child-document-support.md
---



## Context

RFC-010 introduced subfolder documents where `index.md` acts as the sole entrypoint. Other markdown files in the folder are ignored. Authors want to split large documents into companion files (threat models, appendices, detailed criteria) that lazyspec can discover, validate, and navigate. RFC-014 extends folder-based documents so that all `.md` files within a document folder become child documents of the parent.

## Acceptance Criteria

### AC1: Child markdown files are discovered

- **Given** a document folder (e.g. `docs/rfcs/RFC-014-nested/`) contains `index.md` and additional `.md` files
  **When** lazyspec loads the project
  **Then** each additional `.md` file is parsed as a separate document with its own frontmatter

### AC2: Parent-child relationship is tracked

- **Given** a folder contains `index.md` (parent) and `threat-model.md` (child)
  **When** lazyspec loads the project
  **Then** the child is associated with the parent via an internal index, distinct from authored `related` links

### AC3: Virtual parent when no index.md

- **Given** a document folder contains `.md` files but no `index.md`
  **When** lazyspec loads the project
  **Then** a virtual parent is synthesised with title derived from the folder name, type inferred from the prefix, and status set to `draft` (or `accepted` if all children are accepted)

### AC4: Virtual parent is not written to disk

- **Given** a virtual parent has been synthesised
  **When** lazyspec loads and operates on the project
  **Then** no `index.md` file is created on disk for the virtual parent

### AC5: Qualified shorthand resolution

- **Given** a child document exists at `docs/rfcs/RFC-014-nested/threat-model.md`
  **When** a user references `RFC-014/threat-model`
  **Then** shorthand resolution finds and returns the child document

### AC6: Unqualified shorthand does not resolve to children

- **Given** child documents `RFC-014-nested/notes.md` and `RFC-015-other/notes.md` both exist
  **When** a user references just `notes` without a qualifier
  **Then** shorthand resolution does not match either child, avoiding ambiguity

### AC7: Children have their own relationships

- **Given** a child document has `related` fields in its frontmatter pointing to external documents
  **When** lazyspec builds its relationship graph
  **Then** the child's authored relationships are resolved and tracked independently of the parent

### AC8: No recursive nesting

- **Given** a document folder contains a subdirectory (e.g. `docs/rfcs/RFC-014-nested/deep/`)
  **When** lazyspec loads the project
  **Then** the subdirectory within the document folder is ignored

## Scope

### In Scope

- Extending `Store::load()` to discover child `.md` files in document subfolders
- Building `children` and `parent_of` indexes on Store
- Virtual parent synthesis when `index.md` is absent
- Qualified shorthand resolution (`PREFIX/child-name`)
- Child documents participating in the relationship graph via their own frontmatter

### Out of Scope

- CLI command changes (`show`, `context`, `list`, `search` output formatting) -- separate story
- TUI rendering of parent-child trees -- separate story
- `lazyspec create` support for folder-based documents -- separate story
- Non-markdown files in document folders (remain opaque, unchanged from RFC-010)
