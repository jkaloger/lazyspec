---
title: Status Command
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- cli
- agents
related:
- implements: docs/rfcs/RFC-007-agent-native-cli.md
---


## Context

An agent starting work needs to understand the full project state: what documents exist, their statuses, and how they relate. Today this requires multiple `list` calls stitched together. A single `status` command gives the complete picture in one call.

## Acceptance Criteria

### AC1: Full project graph

**Given** a project with documents across multiple types
**When** `lazyspec status` is run
**Then** all documents are output with their frontmatter and relationships

### AC2: JSON output

**Given** a project with documents
**When** `lazyspec status --json` is run
**Then** a JSON object is output with a `documents` array (each element containing frontmatter fields) and a `validation` object containing `errors` and `warnings` arrays

### AC3: Inline validation

**Given** a project with validation errors or warnings
**When** `lazyspec status --json` is run
**Then** the `validation` object includes both errors and warnings without requiring a separate `validate` call

### AC4: Human-readable output

**Given** a project with documents
**When** `lazyspec status` is run without `--json`
**Then** documents are displayed in a compact table format grouped by type, showing title, status, and path

### AC5: Empty project

**Given** an initialised project with no documents
**When** `lazyspec status` is run
**Then** an empty result is output (empty table or empty JSON arrays) with exit code 0

## Scope

### In Scope

- `status` subcommand
- All documents with frontmatter fields
- Inline validation (errors and warnings)
- `--json` flag
- Human-readable grouped table output

### Out of Scope

- Document bodies
- Filtering by type or status (use `list` for that)
- Historical state or changelog
