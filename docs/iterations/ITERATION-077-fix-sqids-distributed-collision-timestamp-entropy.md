---
title: Fix sqids distributed collision - timestamp entropy
type: iteration
status: accepted
author: agent
date: 2026-03-18
tags: []
related:
- implements: STORY-064
---




## Context

AUDIT-004 Finding 1 identified that `next_sqids_id` uses `count_prefixed_files + 1` as the sqids input. Since sqids is deterministic, two users branching from the same state and creating the same document type will always produce identical IDs. This defeats the stated purpose of sqids numbering for distributed workflows.

The fix replaces the count-based input with a seconds-precision Unix timestamp. For a team of 5, collisions require two creates in the exact same second, which is negligible risk. IDs grow from ~3 chars to ~6 chars.

## Changes

### Task 1: Replace count-based input with timestamp in `next_sqids_id`

**Findings addressed:** AUDIT-004 Finding 1

**Files:**
- Modify: `src/engine/template.rs`

**What to implement:**

In `next_sqids_id` (line 86), replace:
```rust
let count = count_prefixed_files(dir, prefix);
let mut input = (count + 1) as u64;
```
with:
```rust
use std::time::{SystemTime, UNIX_EPOCH};
let ts = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("system clock before unix epoch")
    .as_secs();
let mut input = ts;
```

Remove the `count_prefixed_files` function (lines 27-38) since it becomes unused after this change.

Add `use std::time::{SystemTime, UNIX_EPOCH};` to the module imports.

The collision-retry loop (lines 98-106) stays unchanged. It still increments `input` on local filesystem collision.

**How to verify:**
```
cargo test --test sqids_numbering_test
cargo test template::tests
```

### Task 2: Update unit tests for timestamp-based generation

**Findings addressed:** AUDIT-004 Finding 1

**Files:**
- Modify: `src/engine/template.rs` (inline tests)
- Modify: `tests/sqids_numbering_test.rs`

**What to implement:**

Several tests assumed count-based behaviour and need updating:

**Unit tests in `template.rs`:**

- `sqids_collision_retry` (line 198): Currently relies on count=0 producing a predictable ID. After the change, the input is a timestamp so the first ID is unpredictable. Rewrite: generate two IDs in the same directory, assert they differ (the collision-retry loop must still skip existing prefixes).
- `sqids_collision_retry_forced` (line 222): Manually computes `sqids.encode(&[2])` and `sqids.encode(&[3])` expecting count-based inputs. Rewrite: call `next_sqids_id` once to get the first ID, plant a file with that ID prefix, call again and assert the second ID differs. Do not predict the exact ID value.

**Integration tests in `sqids_numbering_test.rs`:**

- `create_retries_on_collision` (line 209): Should still pass as-is (creates two docs sequentially, asserts different IDs). Timestamps will differ between calls. No change needed, but verify.
- `create_handles_preexisting_colliding_file` (line 233): Creates a doc, deletes it, plants a collider with the same ID. After the change, the second create will use a different timestamp entirely, so it won't collide with the planted file at all. Rewrite: plant a file using a known timestamp-derived ID (call `next_sqids_id` to get it), then immediately call again and assert the retry produces a different ID.

**How to verify:**
```
cargo test --test sqids_numbering_test
cargo test template::tests
```

### Task 3: Update RFC-027 trade-off table

**Findings addressed:** AUDIT-004 Finding 3

**Files:**
- Modify: `docs/rfcs/RFC-027-sqids-document-numbering.md`

**What to implement:**

In the trade-off table (line 169), change the "Conflict risk" row from:

| Aspect | Incremental | Sqids |
|--------|-------------|-------|
| Conflict risk | High in distributed workflows | Near zero |

to:

| Aspect | Incremental | Sqids |
|--------|-------------|-------|
| Conflict risk | High in distributed workflows | Negligible (requires same-second create) |

In the "ID Generation" section (line 69), update step 2 from "Count the total number of documents" to "Use the current Unix timestamp (seconds precision) as the sqids input". Update step 3 accordingly.

**How to verify:**

Read the updated RFC and confirm the description matches the implementation.

## Test Plan

- `next_sqids_id` with an empty directory produces a valid lowercase alphanumeric ID (existing test, should pass unchanged)
- `next_sqids_id` called twice in the same directory produces different IDs (timestamp changes between calls, or collision-retry kicks in if same second)
- `next_sqids_id` with a pre-existing file matching the candidate ID retries and produces a different ID
- `min_length` configuration is still respected (timestamp inputs produce longer raw IDs, but min_length still applies)
- Different salts still produce different IDs (salt shuffles the alphabet independently of the input source)
- Integration: `lazyspec create` with sqids config produces non-incremental filenames
- Integration: mixed numbering types still work (sqids on one type, incremental on another)

All tests are deterministic (no timing-sensitive assertions on exact ID values), isolated (temp directories), and fast (no I/O beyond temp filesystem).

## Notes

The `count_prefixed_files` function becomes dead code after this change. It should be removed to keep the module clean. No other callers exist (verified via grep).
