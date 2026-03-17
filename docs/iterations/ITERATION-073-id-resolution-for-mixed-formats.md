---
title: ID resolution for mixed formats
type: iteration
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: docs/stories/STORY-065-id-resolution-for-mixed-formats.md
---



## Test Plan

- `extract_id_from_name("RFC-022-some-title")` returns `"RFC-022"` (AC-1)
- `extract_id_from_name("RFC-k3f-some-title")` returns `"RFC-k3f"` (AC-2)
- `extract_id_from_name("STORY-a2b-some-title")` returns `"STORY-a2b"` (AC-5)
- `resolve_shorthand("RFC-022")` resolves to `RFC-022-foo.md` in a mixed directory (AC-3)
- `resolve_shorthand("RFC-k3f")` resolves to `RFC-k3f-bar.md` in a mixed directory (AC-4)
- `resolve_shorthand` with a non-existent sqids ID returns `NotFound` (AC-6)
- `extract_id` on `RFC-k3f-some-title/index.md` returns `"RFC-k3f"` (AC-7)
- Existing numeric-only tests still pass (regression)

## Changes

### Task 1: Update `extract_id_from_name` to accept alphanumeric ID segments

**ACs addressed:** AC-1, AC-2, AC-5

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**
The current `extract_id_from_name` scans for the first all-digit segment after the prefix. Change the logic: after the uppercase prefix (e.g. `RFC-`, `STORY-`), treat the next segment as the ID regardless of whether it is numeric or alphanumeric. The pattern is `{PREFIX}-{ID}-{slug}`, where PREFIX is one or more uppercase letters and ID is the segment immediately following. Return `parts[..=prefix_end+1].join("-")` where `prefix_end` is the index of the last uppercase-only segment.

Concretely: find the first segment that is NOT all-uppercase. That segment is the ID. Return everything up to and including it. This preserves `RFC-022` (022 is not all-uppercase) and adds `RFC-k3f` (k3f is not all-uppercase). Multi-word prefixes are not used, so the prefix is always index 0.

**How to verify:**
`cargo test` -- all existing store tests pass, plus new tests from Task 3.

### Task 2: Update `strip_type_prefix` to handle alphanumeric IDs

**ACs addressed:** AC-3, AC-4 (indirect, supports title extraction)

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**
`strip_type_prefix` currently expects digits after the prefix dash. Update the digit-scanning loop to also accept lowercase alphanumeric characters (`is_ascii_alphanumeric`). This ensures folder/file names with sqids IDs get their titles extracted correctly.

**How to verify:**
`cargo test` -- title extraction for sqids-named documents works.

### Task 3: Add unit and integration tests for mixed-format ID resolution

**ACs addressed:** AC-1 through AC-7

**Files:**
- Modify: `tests/store_test.rs`

**What to implement:**
Add a `setup_mixed_fixture()` that creates both `RFC-022-foo.md` and `RFC-k3f-bar.md` in the same directory, plus a folder-based `RFC-a1b-folder-doc/index.md`. Add tests:
1. `extract_id_from_name` returns correct IDs for numeric, sqids, and multi-segment cases
2. `resolve_shorthand` finds the right document for both `RFC-022` and `RFC-k3f`
3. `resolve_shorthand` returns `NotFound` for `RFC-zzz`
4. `extract_id` on folder-based sqids path returns `RFC-a1b`

Note: `extract_id_from_name` is already `pub`. `extract_id` is private, so test it indirectly through store loading (check `doc.id` after loading the fixture).

**How to verify:**
`cargo test store` -- all new tests pass.
