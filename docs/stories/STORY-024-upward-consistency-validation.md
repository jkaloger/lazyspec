---
title: Upward Consistency Validation
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- validation
- health
related:
- implements: docs/rfcs/RFC-008-project-health-awareness.md
---



## Context

`validate_full()` checks child-to-parent consistency: rejected parent (error), superseded parent (warning), and orphaned acceptance where an accepted iteration has a draft parent story (warning). It doesn't check the reverse direction. When all stories under an RFC are accepted but the RFC itself is still draft, nothing flags it. This is the most common form of status drift and the one that triggered RFC-008.

The existing `OrphanedAcceptance` check is also narrowly scoped to iteration->story. The same pattern applies to story->RFC.

## Acceptance Criteria

### AC1: All children accepted warning

**Given** an RFC in `draft` or `review` status where every story that `implements` it is `accepted`
**When** `lazyspec validate --warnings` is run
**Then** a warning is reported listing the parent and its accepted children

### AC2: Story-level all children accepted

**Given** a story in `draft` or `review` status where every iteration that `implements` it is `accepted`
**When** `lazyspec validate --warnings` is run
**Then** a warning is reported listing the story and its accepted iterations

### AC3: Generalised orphaned acceptance

**Given** an accepted story whose parent RFC (via `implements`) is still `draft`
**When** `lazyspec validate --warnings` is run
**Then** a warning is reported (extending the existing check which only covers iteration->story)

### AC4: No false positives on partial completion

**Given** an RFC where some stories are `accepted` and others are `draft`
**When** `lazyspec validate --warnings` is run
**Then** no "all children accepted" warning is emitted for that RFC

### AC5: Reverse index construction

**Given** a project with documents linked via `implements`
**When** validation runs
**Then** the reverse index (parent -> children) is built from the existing relationship data without requiring new frontmatter fields

### AC6: JSON output includes new warnings

**Given** upward consistency warnings exist
**When** `lazyspec validate --json` is run
**Then** the new warnings appear in the `warnings` array with a distinct issue type

## Scope

### In Scope

- Reverse-index traversal of `implements` links
- "All children accepted" warning for RFC and story parents
- Generalising `OrphanedAcceptance` to story->RFC
- JSON representation of new warning types

### Out of Scope

- Auto-promoting parent status
- Warnings for `related-to` links (only `implements`)
- Configurable warning thresholds
