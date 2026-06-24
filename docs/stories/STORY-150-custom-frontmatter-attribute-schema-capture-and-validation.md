---
title: Custom frontmatter attribute schema, capture, and validation
type: story
status: accepted
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: RFC-049
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

RFC-049 foundation. `RawFrontmatter` (`document.rs:197`) silently drops unknown keys; there is no way to declare typed attributes (estimate, priority) or validate them. Every downstream slice (sort, columns) depends on this.

## Acceptance Criteria

- **Given** a type declares `[[types.attributes]]` with `name`, `kind`, optional `required`/`values`
  **When** config loads
  **Then** `TypeDef.attributes: Vec<AttrDef>` deserializes, with `AttrKind` one of int/float/string/enum/date/bool

- **Given** a document's frontmatter carries a declared attribute
  **When** the doc parses
  **Then** `DocMeta` exposes it typed in an attribute map; `date` reuses the existing custom deserializer

- **Given** an attribute value of the wrong kind, an enum value outside `values`, or a missing `required` attribute
  **When** `lazyspec validate` runs
  **Then** each is reported as an **error**

- **Given** a frontmatter key not declared on the type
  **When** `lazyspec validate` runs
  **Then** it is reported as a **warning**, and the doc still parses

## Scope

### In Scope

- `AttrDef`, `AttrKind`, `AttrValue` (incl. a raw/untyped variant for undeclared keys — see RFC-049 review note)
- `TypeDef.attributes` deserialization with sensible defaults
- Frontmatter capture into `DocMeta`
- Validation rules wired into `lazyspec validate` at existing error/warning severities

### Out of Scope

- Any TUI or CLI `--json` surfacing (STORY-152)
- Sorting or display of attributes (STORY-154)
