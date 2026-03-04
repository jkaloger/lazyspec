---
title: "Story and Iteration Document Types"
type: story
status: accepted
author: "jkaloger"
date: 2026-03-05
tags: [document-types, search, validation]
related:
  - implements: docs/rfcs/RFC-002-ai-driven-workflow.md
---

## Context

The original Spec/Plan document types need to be renamed to Story/Iteration to better reflect the agent-driven development workflow. This also includes adding a search command, strict validation rules, and updated templates.

## Acceptance Criteria

### AC1: DocType rename

**Given** the codebase uses `DocType::Spec` and `DocType::Plan`
**When** the rename is applied
**Then** `DocType::Story` and `DocType::Iteration` are used throughout, with `type: story` and `type: iteration` in frontmatter

### AC2: Config directory rename

**Given** the config references `specs` and `plans` directories
**When** the rename is applied
**Then** directories are `stories` and `iterations` in both config and on disk

### AC3: Story template

**Given** a user runs `lazyspec create story "<title>"`
**When** the document is created
**Then** the template includes Context, Acceptance Criteria (with given/when/then), and Scope (In/Out) sections

### AC4: Iteration template

**Given** a user runs `lazyspec create iteration "<title>"`
**When** the document is created
**Then** the template includes Changes, Test Plan, and Notes sections

### AC5: Search command

**Given** a project with documents
**When** `lazyspec search <query>` is run
**Then** documents matching the query (case-insensitive substring across title, tags, body) are returned with match field and context snippet

### AC6: Search type filter

**Given** a search query with `--type <type>` flag
**When** the search is run
**Then** results are filtered to only the specified document type

### AC7: Strict validation - unlinked iterations

**Given** an iteration document without an `implements` relation to a story
**When** `lazyspec validate` is run
**Then** an `UnlinkedIteration` validation error is reported

### AC8: Strict validation - unlinked ADRs

**Given** an ADR document with no relations
**When** `lazyspec validate` is run
**Then** an `UnlinkedAdr` validation error is reported

## Scope

### In Scope

- `DocType` enum rename (Spec -> Story, Plan -> Iteration)
- Config and directory rename
- Story and Iteration default templates
- `Store::search` method
- `lazyspec search` CLI subcommand with `--type` and `--json` flags
- `UnlinkedIteration` and `UnlinkedAdr` validation error variants
- Updated tests throughout

### Out of Scope

- Agent skill files (covered by STORY-005)
- New relationship types
- Body content indexing for fuzzy search in TUI
