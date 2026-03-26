---
title: DRY Consolidation
type: story
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: RFC-032
---




## Context

RFC-032 stream 3. Several helper patterns are duplicated across the engine and CLI layers: path extraction logic, display name resolution, TypeDef construction, and type-prefix stripping. This story extracts shared helpers and clarifies naming where consolidation isn't appropriate.

## Acceptance Criteria

### AC1: Path Extraction (Stream 3a)

- Given duplicated path-to-canonical-name logic in resolve_shorthand()'s qualified and unqualified branches
  When the canonical_name() helper from Story 1 (AC4) is available
  Then both branches delegate to it, eliminating the duplication

### AC2: Display Name (Stream 3b)

- Given doc_display_name in fix.rs and extract_id in store.rs both resolving paths by checking for index.md
  When DocMeta::display_name() is added to document.rs
  Then both call-sites use the new method and the ad-hoc path logic in fix.rs is removed

### AC3: TypeDef Construction (Stream 3c)

- Given nearly identical TypeDef construction in default_types() and types_from_directories()
  When a build_type_def() builder function is extracted
  Then both call-sites become one-liners delegating to the builder

### AC4: strip_type_prefix Naming (Stream 3d)

- Given strip_type_prefix in store.rs matching sqids-style characters and strip_type_prefix in fix.rs matching digits only
  When the functions are renamed to strip_type_prefix_sqids and strip_type_prefix_numeric respectively
  Then the naming makes the behavioural difference explicit without false consolidation

## Scope

### In Scope

- canonical_name() adoption (depends on Story 1 delivering the helper)
- DocMeta::display_name() method in document.rs
- build_type_def() extraction
- strip_type_prefix rename in both store.rs and fix.rs

### Out of Scope

- Module splits (Stories 4, 5)
- Function renames beyond strip_type_prefix (Story 3 TUI naming)
