---
title: Context Command
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

Agents need the full design-to-implementation chain (RFC -> Story -> Iteration) before starting work. Today this requires the `resolve-context` skill, which is Claude Code specific. A CLI command makes this available to any agent runtime.

## Acceptance Criteria

### AC1: Basic chain resolution

**Given** a document that implements another document via `implements` relationships
**When** `lazyspec context <id>` is run with a shorthand ID or path
**Then** the full chain is output in order from root (RFC) to leaf (the given document), showing frontmatter for each document

### AC2: No implements links

**Given** a document with no `implements` relationships (e.g. a standalone RFC)
**When** `lazyspec context <id>` is run
**Then** only that document's frontmatter is output

### AC3: JSON output

**Given** any document
**When** `lazyspec context <id> --json` is run
**Then** a JSON object with a `chain` array is output, each element containing the document's frontmatter fields (path, title, type, status, author, date, tags, related)

### AC4: Human-readable output

**Given** any document
**When** `lazyspec context <id>` is run without `--json`
**Then** each document in the chain is printed with title, type, status, and path in a readable format

### AC5: Document not found

**Given** an ID that doesn't match any document
**When** `lazyspec context <id>` is run
**Then** an error message is printed to stderr and exit code is 1

## Scope

### In Scope

- `context` subcommand with shorthand ID resolution
- Walking `implements` links upward (child -> parent)
- `--json` flag
- Human-readable default output

### Out of Scope

- Walking relationships downward (what implements this?)
- Including document bodies in output
- Non-implements relationship types in chain walking
