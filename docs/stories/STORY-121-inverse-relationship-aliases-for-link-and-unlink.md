---
title: Inverse relationship aliases for link and unlink
type: story
status: accepted
author: jkaloger
date: 2026-06-12
tags: []
related:
- implements: RFC-001
---

## Context

The relationship model (ADR-003) stores typed relationships in the source document's `related` array and computes the reverse direction in the link graph. Today the only way to record that A is blocked by B is to know it must be written as `blocks: A` on B, then run `link B blocks A`. The user must mentally invert the relationship and target the other document.

This story adds inverse keywords to the `link` and `unlink` commands so the relationship can be expressed from either end. An inverse keyword is a write-time alias: it resolves to the canonical relation written on the correct document. Nothing new is stored. The four canonical relations gain inverses where the direction is meaningful:

| Canonical      | Inverse           |
| -------------- | ----------------- |
| `implements`   | `implemented-by`  |
| `supersedes`   | `superseded-by`   |
| `blocks`       | `blocked-by`      |
| `related-to`   | (symmetric, none) |

Storing only canonical relations preserves the ADR-003 invariant that the reverse direction is computed, never persisted. A companion ADR records this write-time-alias decision as an amendment to ADR-003.

## Acceptance Criteria

### Forward keywords unchanged

- **Given** documents A and B
  **When** the user runs `link A blocks B`
  **Then** A's `related` gains `blocks: B` and B is unchanged, exactly as before this story.

### Inverse keyword flips direction and stores canonical

- **Given** documents A and B
  **When** the user runs `link A blocked-by B`
  **Then** B's `related` gains `blocks: A`, A is unchanged, and no `blocked-by` key is written to any document.

- **Given** the inverse keywords `implemented-by` and `superseded-by`
  **When** the user links with either against a source and target
  **Then** the canonical relation (`implements`, `supersedes`) is written on the target document with the direction flipped, and the inverse keyword never appears in stored frontmatter.

### Symmetric relation has no distinct inverse

- **Given** the symmetric relation `related-to`
  **When** the user inspects the accepted relationship keywords
  **Then** `related-to` is its own inverse and exposes no separate inverse keyword.

### Unlink mirrors inverse semantics

- **Given** B's `related` contains `blocks: A`
  **When** the user runs `unlink A blocked-by B`
  **Then** the `blocks: A` entry is removed from B, mirroring the inverse direction used by `link`.

### Unknown keywords are rejected at link time

- **Given** a keyword that is neither a canonical relation nor a recognised inverse
  **When** the user runs `link` or `unlink` with it
  **Then** the command fails with a clear error naming the unknown keyword, instead of writing an unparseable relation to frontmatter.

### Command output names the stored relation

- **Given** the user links via an inverse keyword
  **When** the command succeeds
  **Then** the output states the canonical relation that was stored and the document it was written to, so the effect of the flip is visible.

### Discoverability

- **Given** shell completion for the relationship-type argument
  **When** the user requests completions
  **Then** the inverse keywords are offered alongside the canonical ones.

- **Given** the README documents the `link` command
  **When** a reader consults it
  **Then** the inverse keywords and their flip semantics are described.

## Scope

### In Scope

- Inverse keywords on the `link` and `unlink` CLI commands.
- Write-time translation to the canonical relation on the correct document, with no new stored relation types.
- Link-time validation that rejects unknown keywords.
- Command output, shell completions, and README updates for the new keywords.

### Out of Scope

- Persisting inverse relations (the graph computes the reverse direction; ADR-003 invariant holds).
- The TUI relationship-creation form. Adding inverse keywords there is a separate slice.
- Materialising both directions as two stored entries (explicitly rejected; would violate ADR-003).
- Adding or renaming any of the four canonical relation types.
