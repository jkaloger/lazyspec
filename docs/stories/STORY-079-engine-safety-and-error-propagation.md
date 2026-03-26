---
title: Engine Safety and Error Propagation
type: story
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: RFC-032
---




## Context

RFC-032 streams 1, 2a, 2b, 2c. The engine layer has panicking unwrap()/expect() calls in the sqids numbering pipeline and deeply nested functions in validation, store loading, and shorthand resolution. This story replaces panics with error propagation and flattens nesting by extracting focused helpers.

## Acceptance Criteria

### AC1: Error Propagation (Stream 1)

- Given sqids builder or encode calls in template.rs, create.rs, fix.rs, and reservations.rs
  When the sqids configuration is invalid or encoding fails
  Then the error propagates via `?` instead of panicking with `.expect()`

- Given TUI cache lookups (e.g. filtered_docs_cache)
  When the cache entry is missing
  Then the code uses `.unwrap_or_default()` or a match expression instead of `.unwrap()`

### AC2: Flatten validate_full() (Stream 2a)

- Given the validate_full() function in validation
  When validation runs
  Then each concern is handled by a separate function: validate_broken_links(), validate_parent_links(), validate_status_consistency(), validate_duplicate_ids()

- Given the extracted validation helpers
  When each helper runs
  Then it uses early `continue` to skip irrelevant documents and returns a flat Vec of issues

### AC3: Flatten Store::load() (Stream 2b)

- Given Store::load() combining filesystem traversal, parsing, and link building
  When loading documents
  Then load_type_directory() handles per-type entry iteration and parse_document_entry() handles single-file read-parse-validate

- Given the refactored Store::load()
  When virtual doc creation runs
  Then it remains in load() since it depends on the full document set

### AC4: Flatten resolve_shorthand() (Stream 2c)

- Given duplicated 3-level nested closures in qualified and unqualified branches
  When resolving shorthand references
  Then a canonical_name(doc: &DocMeta) helper handles name extraction for both branches

- Given canonical_name()
  When the path ends in index.md or .virtual
  Then it returns the parent directory name or filename accordingly

## Scope

### In Scope

- All sqids .expect() calls replaced with ? propagation
- TUI .unwrap() calls on cache lookups replaced with safe alternatives
- validate_full() split into per-concern helpers
- Store::load() split into load_type_directory() and parse_document_entry()
- resolve_shorthand() flattened via canonical_name() helper

### Out of Scope

- Comprehensive unwrap() audit beyond sqids and TUI cache (deferred)
- Module splits (Story 5)
- Function renames beyond what's needed for extraction (Story 3)
