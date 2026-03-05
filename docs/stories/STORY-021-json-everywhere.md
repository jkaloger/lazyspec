---
title: "JSON Everywhere"
type: story
status: draft
author: "jkaloger"
date: 2026-03-05
tags: [cli, agents, json]
related:
  - implements: docs/rfcs/RFC-007-agent-native-cli.md
---

## Context

Agents parse CLI output programmatically. Some commands support `--json` (list, search, validate) but others don't (show). The document schema also varies between commands. Consistent JSON output on all commands with a shared document schema removes parsing ambiguity.

## Acceptance Criteria

### AC1: Show supports JSON

**Given** a document path or shorthand ID
**When** `lazyspec show <id> --json` is run
**Then** a JSON object is output containing the document's frontmatter fields and a `body` field with the markdown content

### AC2: Consistent document schema

**Given** any command that outputs document information with `--json`
**When** the output is parsed
**Then** document objects share the same schema: path, title, type, status, author, date, tags, related

### AC3: Create supports JSON

**Given** a valid document type and title
**When** `lazyspec create <type> <title> --json` is run
**Then** a JSON object is output containing the created document's path and frontmatter

### AC4: Context supports JSON

**Given** a document with implements relationships
**When** `lazyspec context <id> --json` is run
**Then** the chain array uses the same document schema as other commands

## Scope

### In Scope

- `--json` on `show` and `create`
- Consistent document schema across list, search, show, context, status
- Body included only in `show --json` (all others are frontmatter-only)

### Out of Scope

- JSON output on `update`, `delete`, `link`, `unlink` (mutation commands with simple confirmation output)
- Changing existing JSON output formats on list, search, validate
