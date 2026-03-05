---
title: Summary View
type: story
status: draft
author: jkaloger
date: 2026-03-05
tags: [cli, status, health]
related:
- implements: docs/rfcs/RFC-008-project-health-awareness.md
---


## Context

Agents starting work on a project need a quick pulse check: how many documents exist, what's their status breakdown, and are there any health issues. The current `status` command lists every document individually, which is noisy for large projects and expensive in tokens. A summary view gives the high-level picture in a few lines.

## Acceptance Criteria

### AC1: Summary flag

**Given** a project with documents
**When** `lazyspec status --summary` is run
**Then** output shows counts grouped by document type, broken down by status (e.g. "RFC  6 accepted  1 draft")

### AC2: Health line

**Given** a project with validation warnings or errors
**When** `lazyspec status --summary` is run
**Then** a health line appears showing error and warning counts with a hint to run `validate --warnings` for details

### AC3: Clean health

**Given** a project with no validation issues
**When** `lazyspec status --summary` is run
**Then** the health line is omitted or shows a clean state

### AC4: JSON summary output

**Given** a project with documents
**When** `lazyspec status --summary --json` is run
**Then** output is a JSON object with `counts` (type -> status -> count) and `health` (errors/warnings counts)

### AC5: Zero counts omitted

**Given** a project with no documents of a particular status
**When** `lazyspec status --summary` is run (human-readable)
**Then** zero-count statuses are not shown (e.g. if no rejected documents, "rejected" doesn't appear)

### AC6: Mutually exclusive with tree

**Given** the `--summary` and `--tree` flags
**When** both are provided
**Then** the CLI reports an error and does not produce output

## Scope

### In Scope

- `--summary` flag on `status` command
- Human-readable compact table
- JSON counts output
- Inline health summary from validation
- Mutual exclusion with `--tree`

### Out of Scope

- Historical trend data (counts over time)
- Per-RFC rollup (that's closer to tree view territory)
- Sparklines or visual charts
