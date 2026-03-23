---
title: Engine Module Splits
type: story
status: draft
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/rfcs/RFC-032-code-quality-remediation-audit-007-findings.md
---


## Context

RFC-032 streams 5e and 5f. store.rs (604 lines) and refs.rs (425 lines) each combine multiple concerns. This story splits them into focused submodules. Should follow Stories 1-4 so the splits target the already-refactored code.

## Acceptance Criteria

### AC1: Split store.rs (Stream 5e)

- Given store.rs containing Store struct, query API, loading, and link traversal
  When the module is split
  Then store/ contains loader.rs (load, load_type_directory, parse_document_entry) and links.rs (forward_links, reverse_links, related_to, referenced_by)

- Given the split store/ module structure
  When store.rs serves as the module root
  Then it contains the Store struct and query API (list, get, resolve_shorthand), re-exporting submodule items as needed

### AC2: Split refs.rs (Stream 5f)

- Given refs.rs containing RefExpander, expansion logic, code fence detection, and resolution helpers
  When the module is split
  Then refs/ contains code_fence.rs (find_fenced_code_ranges) and resolve.rs (resolve_ref, resolve_head_short_sha, language_from_extension)

- Given the split refs/ module structure
  When refs.rs serves as the module root
  Then it contains RefExpander and expand/expand_cancellable

### AC3: Behavioural Equivalence

- Given all existing engine tests
  When the splits are complete
  Then all tests pass without modification to assertions (only import paths change)

## Scope

### In Scope

- Split store.rs into store/ with loader.rs and links.rs
- Split refs.rs into refs/ with code_fence.rs and resolve.rs
- Update import paths across the codebase

### Out of Scope

- TUI module splits (Story 3)
- CLI module splits (Story 4)
- SOLID refactors (Story 6)
- New engine functionality
