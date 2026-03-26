---
title: CLI Fix Module Restructure
type: story
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: RFC-032
---




## Context

RFC-032 streams 4b and 5d. cli/fix.rs is 1043 lines handling field fixing, conflict resolution, renumbering, reference scanning, and output formatting. The function names use "collect" which obscures the planning semantics. This story renames for clarity and splits the module.

## Acceptance Criteria

### AC1: Function Renames (Stream 4b)

- Given collect_renumber_fixes() in fix.rs
  When the function is renamed to plan_renumbering()
  Then the name conveys that it produces a list of intended changes without applying them

- Given collect_all() in fix.rs
  When the function is renamed to plan_field_and_conflict_fixes()
  Then the name conveys planning semantics matching the dry-run behaviour

### AC2: Module Split (Stream 5d)

- Given fix.rs at 1043 lines with multiple concerns
  When the module is split
  Then fix/ contains fields.rs (field fix collection and application), conflicts.rs (duplicate ID detection and resolution), renumber.rs (renumbering orchestration and reference cascade), and output.rs (JSON and human-readable formatting)

- Given the split fix/ module structure
  When fix.rs serves as the module root
  Then it contains only the run() and run_json() entry points, delegating to submodules

### AC3: Behavioural Equivalence

- Given all existing fix tests
  When the renames and split are complete
  Then all tests pass without modification to assertions (only import paths change)

## Scope

### In Scope

- Rename collect_renumber_fixes -> plan_renumbering
- Rename collect_all -> plan_field_and_conflict_fixes
- Split fix.rs into fix/ with fields.rs, conflicts.rs, renumber.rs, output.rs
- Update import paths across the codebase

### Out of Scope

- Engine module splits (Story 5)
- SOLID refactors (Story 6)
- New fix functionality
