---
title: "CLI Commands"
type: story
status: accepted
author: "jkaloger"
date: 2026-03-04
tags: [cli, commands]
related:
  - implements: docs/rfcs/RFC-001-my-first-rfc.md
---

## Context

lazyspec needs a CLI interface that lets users and agents create, query, modify, and validate documents from the terminal. Commands compose engine primitives and support `--json` output for machine consumption.

## Acceptance Criteria

### AC1: Init

**Given** a directory without a `.lazyspec.toml` config
**When** `lazyspec init` is run
**Then** a default config file is created and all document directories are created

### AC2: Create

**Given** a valid document type and title
**When** `lazyspec create <type> <title>` is run
**Then** a new markdown file is created from the appropriate template with auto-generated filename, and the path is printed to stdout

### AC3: List

**Given** a project with documents
**When** `lazyspec list [type] [--status X]` is run
**Then** matching documents are listed with path, title, type, and status

### AC4: Show

**Given** a document path or shorthand ID (e.g. `RFC-001`)
**When** `lazyspec show <path-or-id>` is run
**Then** the full document content is printed to stdout

### AC5: Update

**Given** a document path and a field to update (e.g. `--status accepted`)
**When** `lazyspec update <path> --status X` is run
**Then** the frontmatter field is updated in the file on disk

### AC6: Delete

**Given** a document path
**When** `lazyspec delete <path>` is run
**Then** the file is removed from disk

### AC7: Link and Unlink

**Given** two document paths and a relation type
**When** `lazyspec link <from> <rel> <to>` is run
**Then** a typed relationship is added to the source document's frontmatter

**Given** an existing relationship
**When** `lazyspec unlink <from> <rel> <to>` is run
**Then** the relationship is removed from the source document's frontmatter

### AC8: Validate

**Given** a project with documents
**When** `lazyspec validate` is run
**Then** validation errors are printed and the exit code reflects pass (0) or fail (2)

### AC9: JSON output

**Given** any list or show command with `--json` flag
**When** the command is run
**Then** output is valid JSON suitable for agent consumption

## Scope

### In Scope

- All CLI subcommands: init, create, list, show, update, delete, link, unlink, validate
- Clap derive-based argument parsing
- Shorthand ID resolution (e.g. `RFC-001` matches by filename prefix)
- `--json` flag on applicable commands
- Exit codes: 0 success, 1 not found, 2 validation error

### Out of Scope

- TUI interface
- Search command (covered by STORY-004)
- Strict validation rules for iterations/ADRs (covered by STORY-004)
