---
title: Frontmatter Utility Extraction
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags:
- refactor
related:
- implements: RFC-012
---




## Context

Five locations across the codebase independently read a markdown file, parse
its YAML frontmatter, modify the YAML, and reconstruct the file using
`format!("---\n{}---\n{}", new_yaml, body)`. This duplication means a
formatting fix or edge-case handling must be applied in five places.

## Acceptance Criteria

- **Given** the engine module
  **When** a caller needs to modify a document's frontmatter
  **Then** a shared utility function handles the read-parse-modify-write cycle

- **Given** `cli/ignore.rs`, `cli/link.rs`, and `tui/app.rs`
  **When** they modify frontmatter
  **Then** they delegate to the shared utility instead of inlining the pattern

- **Given** any existing CLI or TUI operation that modifies frontmatter
  **When** the refactor is complete
  **Then** behaviour is identical to before (no user-visible change)

## Scope

### In Scope

- Shared frontmatter rewrite utility in the engine module
- Replacing all 5 call-sites that use the `format!("---\n{}---\n{}")` pattern

### Out of Scope

- Changing frontmatter format or adding new frontmatter fields
- Modifying the document parsing logic in `engine/document.rs`
- Adding new CLI commands or TUI features
