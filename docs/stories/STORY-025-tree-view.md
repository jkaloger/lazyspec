---
title: Tree View
type: story
status: draft
author: jkaloger
date: 2026-03-05
tags: [cli, status, health]
related:
- implements: docs/rfcs/RFC-008-project-health-awareness.md
---


## Context

`status` groups documents flat by type (RFC, Story, Iteration, ADR). When auditing project health, you need to see which stories belong to which RFC and whether statuses are coherent across each branch. The flat view forces manual cross-referencing. `context` shows a single chain for one document but can't show the full project hierarchy.

## Acceptance Criteria

### AC1: Tree flag

**Given** a project with documents linked via `implements`
**When** `lazyspec status --tree` is run
**Then** documents are displayed in a hierarchy: RFCs at the root, stories indented under their RFC, iterations indented under their story

### AC2: Orphaned documents

**Given** documents with no `implements` link pointing to them and no `implements` link from them (e.g. standalone iterations, unlinked ADRs)
**When** `lazyspec status --tree` is run
**Then** orphaned documents appear in a separate "(orphaned)" section at the bottom

### AC3: Root identification

**Given** documents of any type
**When** building the tree
**Then** root nodes are documents that no other document `implements` (typically RFCs, but also any unparented story or iteration)

### AC4: JSON nested output

**Given** a project with documents
**When** `lazyspec status --tree --json` is run
**Then** output is a JSON object with `roots` (array of nested document trees) and `orphaned` (flat array)

### AC5: Status displayed per node

**Given** the tree output (human-readable)
**When** viewing any node
**Then** the document title and status are visible on the same line

### AC6: Mutually exclusive with summary

**Given** the `--tree` and `--summary` flags
**When** both are provided
**Then** the CLI reports an error and does not produce output

## Scope

### In Scope

- `--tree` flag on `status` command
- Human-readable indented tree output
- JSON nested tree output
- Orphaned document section
- Mutual exclusion with `--summary`

### Out of Scope

- ADR placement in tree (they use `related-to`, not `implements`)
- Colored/styled tree connectors (can follow in a styling iteration)
- Interactive tree navigation
