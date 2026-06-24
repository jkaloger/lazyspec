---
title: Expose document attributes in show and status --json
type: story
status: accepted
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: RFC-049
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

RFC-049, principle 2: agents consume the same interface as humans. Once attributes are captured (STORY-150), they must appear in machine-readable output so agents and the TUI read them the same way.

## Acceptance Criteria

- **Given** a document with declared attributes
  **When** `lazyspec show <id> --json` runs
  **Then** the output includes an `attributes` object with typed values

- **Given** documents with attributes
  **When** `lazyspec status --json` runs
  **Then** each document entry includes its `attributes`

- **Given** a document with no attributes
  **When** either command runs with `--json`
  **Then** `attributes` is present and empty (stable shape for consumers)

## Scope

### In Scope

- Serialize `DocMeta` attributes into `show` and `status` `--json`
- README update for the new field

### Out of Scope

- Attribute schema/validation (STORY-150)
- TUI rendering (STORY-153, STORY-154)
