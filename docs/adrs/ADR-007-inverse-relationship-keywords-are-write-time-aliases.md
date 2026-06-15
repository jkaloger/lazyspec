---
title: Inverse relationship keywords are write-time aliases
type: adr
status: accepted
author: jkaloger
date: 2026-06-12
tags: []
related:
- related-to: ADR-003
- related-to: STORY-121
---

## Context

ADR-003 established four typed relationships (`implements`, `supersedes`, `blocks`, `related-to`) stored in the source document's `related` array, with the reverse direction computed by the link graph rather than persisted.

Expressing a relationship still requires the author to target the correct document with the correct canonical relation. To record "A is blocked by B" the author must know to write `blocks: A` on B. We want inverse keywords (`blocked-by`, `implemented-by`, `superseded-by`) so the relationship reads naturally from either end.

Two ways to honour an inverse keyword:

1. **Materialise both directions.** `link A blocked-by B` stores `blocked-by: A` on B in addition to (or instead of) the canonical entry. This introduces stored inverse relations.
2. **Write-time alias.** `link A blocked-by B` resolves to the canonical `blocks: A` written on B. The keyword is a CLI convenience that flips the direction and the relation name; nothing inverse is stored.

## Decision

Inverse keywords are write-time aliases. The `link` and `unlink` commands translate an inverse keyword to its canonical relation and write it on the target document with the direction flipped. Storage holds only the four canonical relations from ADR-003.

`related-to` is symmetric and is its own inverse, so it has no distinct inverse keyword.

This amends ADR-003 by adding inverse vocabulary to the command surface while leaving the stored schema and the "reverse is computed, never persisted" invariant unchanged.

## Consequences

- The stored frontmatter schema does not change. Parsing, validation, and the link graph are untouched; the reverse direction is still computed.
- There is no second entry to keep in sync on `unlink`, and no risk of the two directions disagreeing.
- Translation is domain knowledge, so the keyword-to-canonical mapping lives in the engine on the relation type, reusable by any consumer (CLI today, TUI later).
- Inverse keywords never appear in stored documents, so a reader of raw frontmatter only ever sees canonical relations. The naturalness is at the command line, not in the file.
- A consumer that wants to offer inverse keywords (e.g. the TUI relationship form) must opt in by using the shared mapping; it does not come for free from the stored data.
