---
title: Frontmatter fix command
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: RFC-015
---




## Context

Once parse errors are visible (STORY-044), users need a way to fix broken documents without manually editing YAML. This story adds a `lazyspec fix` command that fills in missing required frontmatter fields with sensible defaults.

## Acceptance Criteria

### AC1: Fix fills missing fields with defaults

- **Given** a document is missing required frontmatter fields (e.g. no `status`, no `tags`)
  **When** the user runs `lazyspec fix <path>`
  **Then** the missing fields are added with default values (status: draft, tags: [], date: today, author from git config) and existing fields are preserved

### AC2: Fix preserves document body

- **Given** a document has markdown content below the frontmatter
  **When** the user runs `lazyspec fix <path>`
  **Then** the body content is unchanged after the fix

### AC3: Dry run shows changes without writing

- **Given** a document with missing frontmatter fields
  **When** the user runs `lazyspec fix --dry-run <path>`
  **Then** the output shows what fields would be added, but the file on disk is unchanged

### AC4: Fix all documents with no path argument

- **Given** multiple documents have parse errors
  **When** the user runs `lazyspec fix` with no path arguments
  **Then** all documents with parse errors are fixed

### AC5: JSON output

- **Given** a document that needs fixing
  **When** the user runs `lazyspec fix --json <path>`
  **Then** the output is structured JSON with the path, fields added, and whether the file was written

### AC6: Type inference from directory

- **Given** a document in the `rfcs/` directory that is missing the `type` field
  **When** the user runs `lazyspec fix <path>`
  **Then** the `type` field is set based on the configured directory mapping (e.g. `rfcs/` -> `rfc`)

## Scope

### In Scope

- `lazyspec fix` subcommand with `--dry-run` and `--json` flags
- Default value inference for all required fields
- Fixing specific files by path or all broken files at once
- Handling files with no frontmatter delimiters (wrap content with generated block)

### Out of Scope

- Interactive field prompting
- Fixing semantic issues (broken links, invalid status transitions)
- TUI integration (STORY-046)
