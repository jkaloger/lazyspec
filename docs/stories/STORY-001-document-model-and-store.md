---
title: "Document Model and Store"
type: story
status: accepted
author: "jkaloger"
date: 2026-03-04
tags: [engine, parsing, store]
related:
  - implements: docs/rfcs/RFC-001-my-first-rfc.md
---

## Context

lazyspec needs a core data layer that parses markdown files with YAML frontmatter, indexes them in memory, resolves typed relationships between documents, and validates link integrity. This is the foundation that both CLI and TUI consume.

## Acceptance Criteria

### AC1: Frontmatter parsing

**Given** a markdown file with a YAML frontmatter block (delimited by `---`)
**When** the file is loaded by the store
**Then** the title, type, status, author, date, tags, and related fields are parsed into a `DocMeta` struct

### AC2: Document type support

**Given** a document with `type: rfc`, `type: adr`, `type: spec`, or `type: plan`
**When** the type field is parsed
**Then** it maps to the corresponding `DocType` enum variant

### AC3: Store loading

**Given** a project root with configured doc directories containing markdown files
**When** `Store::load` is called
**Then** all documents across all configured directories are indexed by path, with only frontmatter parsed (body loaded lazily)

### AC4: Link graph resolution

**Given** documents with `related` entries containing typed relationships
**When** the store builds its link graph
**Then** both forward and reverse lookups are available (e.g. querying what implements a given RFC)

### AC5: Filtering and listing

**Given** a loaded store with documents of various types and statuses
**When** `Store::list` is called with a filter
**Then** only documents matching the filter criteria (type, status) are returned

### AC6: Validation

**Given** a store with documents containing `related` entries
**When** `Store::validate` is called
**Then** broken links (targets that don't resolve to existing documents) are reported as validation errors

## Scope

### In Scope

- `DocMeta` struct with all frontmatter fields
- `DocType` and `Status` enums with serde deserialization
- `Relation` struct with `RelationType` enum
- `Store` with load, list, get, get_body, related_to, and validate methods
- `LinkGraph` with forward and reverse lookups
- `Config` struct with directory, template, and naming settings
- Template rendering and filename generation

### Out of Scope

- CLI command implementations
- TUI rendering
- Full-text search
- File watching
