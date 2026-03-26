---
title: Graceful degradation for duplicate IDs
type: story
status: accepted
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: RFC-020
---




## Context

When distributed teams create documents on separate branches, numbering conflicts occur after merge (e.g. two `RFC-020` documents). Currently `resolve_shorthand()` silently returns whichever document the HashMap iterator yields first, and the TUI/CLI commands have no awareness of duplicates. This story makes the system usable while conflicts exist by surfacing ambiguity rather than hiding it.

## Acceptance Criteria

- **Given** two documents with the same numeric ID prefix (e.g. `RFC-020-foo.md` and `RFC-020-bar.md`)
  **When** `resolve_shorthand("RFC-020")` is called
  **Then** it returns an error indicating ambiguity, listing all matching document paths

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec show RFC-020` is run
  **Then** the CLI prints an error listing the conflicting document paths and instructs the user to specify by full path

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec show RFC-020 --json` is run
  **Then** the output is a JSON error object containing an `ambiguous_matches` array with the conflicting document paths

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec show docs/rfcs/RFC-020-foo.md` is run (full path)
  **Then** the correct document is returned without ambiguity errors

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec list` is run
  **Then** both documents appear in the output and neither is hidden or merged

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec list --json` is run
  **Then** both documents appear in the JSON array output

- **Given** two documents with the same numeric ID prefix
  **When** `lazyspec context` is run
  **Then** both documents are included in the context output without crashing

- **Given** two documents with the same numeric ID prefix
  **When** the TUI document list is displayed
  **Then** both documents are shown, each flagged with a warning indicator (e.g. a visual marker denoting duplicate ID)

- **Given** a single document with a unique ID prefix
  **When** `resolve_shorthand` is called with that ID
  **Then** the document is returned as before (no regression)

## Scope

### In Scope

- Update `resolve_shorthand()` to detect when multiple documents match a shorthand ID and return an ambiguity error instead of silently picking one
- Update `lazyspec show` to handle the ambiguity error by listing conflicting paths (both human-readable and `--json` modes)
- Ensure `lazyspec list` and `lazyspec context` display all loaded documents including duplicates without crashing or hiding any
- Update TUI document list to visually flag documents that share a numeric ID prefix
- Full-path lookups must still work unambiguously when shorthand is ambiguous

### Out of Scope

- Conflict detection, renumbering, and file renaming on disk (Story 1)
- Reference cascade after renumbering (Story 2)
- Validation diagnostic for duplicate IDs (Story 4)
- Prevention of conflicts at `create` time
