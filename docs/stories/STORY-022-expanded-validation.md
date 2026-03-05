---
title: "Expanded Validation"
type: story
status: draft
author: "jkaloger"
date: 2026-03-05
tags: [cli, agents, validation]
related:
  - implements: docs/rfcs/RFC-007-agent-native-cli.md
---

## Context

Validation currently checks broken links, unlinked iterations, and unlinked ADRs. It doesn't catch semantic inconsistencies across the relationship graph, like accepted work under a superseded parent. Agents and CI need these signals to avoid building on stale foundations.

## Acceptance Criteria

### AC1: Superseded parent warning

**Given** an accepted document that implements a superseded document
**When** `lazyspec validate` is run
**Then** a warning is reported indicating the parent has been superseded

### AC2: Rejected parent error

**Given** an accepted or draft document that implements a rejected document
**When** `lazyspec validate` is run
**Then** an error is reported indicating the parent has been rejected

### AC3: Orphaned acceptance warning

**Given** an accepted iteration whose parent story is still in draft
**When** `lazyspec validate` is run
**Then** a warning is reported indicating the iteration is accepted but its parent story is not

### AC4: Warning vs error severity

**Given** validation results with both warnings and errors
**When** `lazyspec validate` is run
**Then** only errors affect the exit code (exit 2). Warnings alone result in exit 0.

### AC5: Warnings flag

**Given** validation results with warnings
**When** `lazyspec validate` is run without `--warnings`
**Then** only errors are displayed
**When** `lazyspec validate --warnings` is run
**Then** both errors and warnings are displayed

### AC6: JSON output includes severity

**Given** validation results
**When** `lazyspec validate --json` is run
**Then** the output contains separate `errors` and `warnings` arrays

## Scope

### In Scope

- New validation rules: superseded parent, rejected parent, orphaned acceptance
- Warning/error severity distinction
- `--warnings` flag
- Updated JSON output with separate arrays
- Integration with `status --json` inline validation

### Out of Scope

- Git-based staleness detection (date comparisons via git history)
- Custom validation rules or configuration
- Auto-fixing validation issues
