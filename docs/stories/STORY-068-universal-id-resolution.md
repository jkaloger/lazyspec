---
title: Universal ID Resolution
type: story
status: accepted
author: jkaloger
date: 2026-03-20
tags: []
related:
- implements: RFC-028
---




## Context

Referencing documents in lazyspec currently requires full relative paths for commands like `link`, `unlink`, `delete`, `update`, `ignore`, and `unignore`. Meanwhile, `show` and `context` already accept shorthand IDs (e.g. `RFC-001`) via `resolve_shorthand_or_path`. This inconsistency adds friction. The resolution helper should be shared across all path-accepting commands.

## Acceptance Criteria

### Shared resolution helper

- **Given** the `resolve_shorthand_or_path` function exists in `show.rs`
  **When** the codebase is refactored
  **Then** the function is moved to a shared module accessible by all CLI commands, with no duplication

### `link` command shorthand resolution

- **Given** a user runs `lazyspec link STORY-068 implements RFC-028`
  **When** both shorthand IDs resolve to exactly one document each
  **Then** the link is created with canonical paths stored in frontmatter (not shorthand IDs)

- **Given** a user runs `lazyspec link docs/stories/STORY-068-universal-id-resolution.md implements docs/rfcs/RFC-028-document-reference-ergonomics.md`
  **When** full paths are provided
  **Then** the link is created as before (backwards compatible)

### `unlink` command shorthand resolution

- **Given** a user runs `lazyspec unlink STORY-068 implements RFC-028`
  **When** both shorthand IDs resolve to exactly one document each
  **Then** the relationship is removed using the resolved canonical paths

### `delete`, `update`, `ignore`, `unignore` shorthand resolution

- **Given** a user runs any of `delete`, `update`, `ignore`, or `unignore` with a shorthand ID (e.g. `STORY-068`)
  **When** the shorthand resolves to exactly one document
  **Then** the command operates on the resolved document, identical to passing the full path

### Error handling: ambiguous ID

- **Given** a shorthand ID prefix matches multiple documents (e.g. `STORY-0` matches `STORY-001` and `STORY-002`)
  **When** any command attempts resolution
  **Then** the command fails with an error listing the ambiguous matches

### Error handling: not-found ID

- **Given** a shorthand ID matches no documents (e.g. `RFC-999`)
  **When** any command attempts resolution
  **Then** the command fails with a clear "not found" error message

### Backwards compatibility

- **Given** a user passes a full relative path to any affected command
  **When** the path exists on disk
  **Then** the command behaves identically to before this change

## Scope

### In Scope

- Move `resolve_shorthand_or_path` from `show.rs` to a shared location
- Update `link` and `unlink` to accept shorthand IDs for both FROM and TO arguments, resolving to canonical paths at write time
- Update `delete`, `update`, `ignore`, `unignore` to accept shorthand IDs for their path argument
- Error messages for ambiguous and not-found shorthand IDs

### Out of Scope

- Shell completions (covered by Story 2)
- TUI changes (covered by Story 3)
- Changes to the underlying `Store::resolve_shorthand` implementation
- New shorthand formats or aliases
