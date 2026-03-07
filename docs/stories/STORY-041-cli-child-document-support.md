---
title: CLI child document support
type: story
status: draft
author: jkaloger
date: 2026-03-07
tags: []
related:
- implements: docs/rfcs/RFC-014-nested-child-document-support.md
---


## Context

STORY-040 adds engine-level discovery of child documents within folder-based documents. This story ensures the CLI commands surface that parent-child structure correctly. Each command needs awareness of the `children`/`parent_of` indexes so users can navigate between parents and children through the CLI.

## Acceptance Criteria

### AC1: Show parent lists children

- **Given** a parent document has child documents in its folder
  **When** the user runs `lazyspec show <parent-id>`
  **Then** the output includes the parent's content followed by a "Children" section listing each child's title and qualified shorthand

### AC2: Show child indicates parent

- **Given** a child document exists within a folder-based document
  **When** the user runs `lazyspec show <child-id>` (using qualified shorthand)
  **Then** the output includes the child's content and a note indicating which document is its parent

### AC3: Context includes children as relationships

- **Given** a parent document has child documents
  **When** the user runs `lazyspec context <parent-id>`
  **Then** children appear as relationships in the context output, consistent with how `implements` links are shown

### AC4: List includes child documents

- **Given** a project contains folder-based documents with children
  **When** the user runs `lazyspec list`
  **Then** child documents appear in the list alongside top-level documents

### AC5: Search matches child content independently

- **Given** a child document contains a matching term but its parent and siblings do not
  **When** the user runs `lazyspec search "<term>"`
  **Then** only the matching child is returned, not the parent or siblings

### AC6: Validate checks children independently

- **Given** a child document has invalid frontmatter
  **When** the user runs `lazyspec validate`
  **Then** the validation error references the child document specifically

### AC7: JSON output includes parent-child metadata

- **Given** a parent document has children
  **When** the user runs any command with `--json`
  **Then** the JSON output includes the parent-child relationship information (children list on parent, parent reference on child)

## Scope

### In Scope

- `show` output formatting for parents and children
- `context` rendering of folder-containment relationships
- `list` inclusion of child documents
- `search` matching against child documents independently
- `validate` reporting on child documents
- JSON output for all commands reflecting parent-child structure

### Out of Scope

- Engine-level discovery (STORY-040)
- TUI rendering (STORY-042)
- `create` command changes (STORY-043)
