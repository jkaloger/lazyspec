---
title: Store parse error collection
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: docs/rfcs/RFC-015-lenient-frontmatter-loading-with-warnings-and-fix-command.md
---



## Context

`Store::load()` silently drops documents that fail frontmatter parsing. Users have no way to know that a document exists on disk but isn't being loaded. This story adds error tracking to the Store so parse failures become visible through existing CLI commands.

## Acceptance Criteria

### AC1: Parse errors are collected

- **Given** a document directory contains a markdown file with invalid frontmatter (e.g. missing `status` field)
  **When** `Store::load()` runs
  **Then** the file's path and error message are stored in a parse errors collection, and loading continues for remaining files

### AC2: Valid documents still load normally

- **Given** a mix of valid and invalid documents in the same directory
  **When** `Store::load()` runs
  **Then** all valid documents load successfully, and only invalid documents appear in the parse errors collection

### AC3: Validate reports parse errors

- **Given** the store contains parse errors from loading
  **When** the user runs `lazyspec validate --json`
  **Then** the output includes a `parse_errors` array with each entry containing `path` and `error` fields

### AC4: Status includes parse error count

- **Given** the store contains parse errors from loading
  **When** the user runs `lazyspec status --json`
  **Then** the output includes a `parse_errors` array listing the failed documents

## Scope

### In Scope

- Parse error collection in Store
- Surfacing errors in `validate` and `status` JSON output
- Handling all document load paths (flat files, index.md in folders, child documents)

### Out of Scope

- Fixing broken documents (STORY-045)
- TUI display of errors (STORY-046)
- Changing which fields are required in frontmatter
