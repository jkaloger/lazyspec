---
title: Upward Consistency Validation
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-024-upward-consistency-validation.md
---



## Changes

### Task 1: Add `AllChildrenAccepted` and `UpwardOrphanedAcceptance` variants to `ValidationIssue`

**ACs addressed:** AC1, AC2, AC3, AC6

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

Add two new variants to the `ValidationIssue` enum:

```rust
AllChildrenAccepted {
    parent: PathBuf,
    children: Vec<PathBuf>,
},
UpwardOrphanedAcceptance {
    path: PathBuf,
    parent: PathBuf,
},
```

`AllChildrenAccepted` fires when every document that `implements` a parent is `accepted` but the parent is `draft` or `review`. This covers AC1 (story->RFC) and AC2 (iteration->story).

`UpwardOrphanedAcceptance` generalises the existing `OrphanedAcceptance` check. Currently `OrphanedAcceptance` only fires for accepted iterations with draft parent stories. The new variant fires for accepted stories with a draft parent RFC (AC3). Keep the existing `OrphanedAcceptance` variant unchanged to avoid breaking existing tests.

Add `Display` implementations for both:
- `AllChildrenAccepted`: `"all children accepted but parent is draft: {parent} ({n} children)"`
- `UpwardOrphanedAcceptance`: `"accepted but parent not accepted: {path} -> {parent}"`

**How to verify:**
```
cargo test --lib
```

---

### Task 2: Implement reverse-index checks in `validate_full()`

**ACs addressed:** AC1, AC2, AC3, AC4, AC5

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

Add a new block at the end of `validate_full()`, after the existing per-document loop. This block uses the existing `self.reverse_links` HashMap to walk parent->children.

For each document in `self.docs`:

1. Look up the document's path in `self.reverse_links` to get all documents that reference it.
2. Filter to only `Implements` relationships.
3. If there are zero `implements` children, skip (nothing to check).
4. Collect the children's statuses by looking up each child path in `self.docs`.
5. **All children accepted check (AC1, AC2):** If every child has `status == Accepted` and the parent is `Draft` or `Review`, push an `AllChildrenAccepted` warning with the parent path and list of children paths. Only check meaningful pairs: RFC parents with Story children, Story parents with Iteration children.
6. **Generalised orphaned acceptance (AC3):** For each child that is `Accepted` where the parent is `Draft` or `Review` (but not all children are accepted -- that's covered above): if the child is a Story and the parent is an RFC, push `UpwardOrphanedAcceptance`. This extends the existing iteration->story check to story->RFC.

AC4 (no false positives) is satisfied by step 5's "every child" requirement. If any child is not accepted, no `AllChildrenAccepted` warning fires.

AC5 (reverse index) is satisfied by using `self.reverse_links` which is already built during `Store::load()`.

**How to verify:**
```
cargo test
```

---

### Task 3: Update JSON and human output for new warning types

**ACs addressed:** AC6

**Files:**
- Modify: `src/cli/validate.rs`

**What to implement:**

No changes needed to `run_json()` or `run_human()` -- they already iterate over `result.warnings` and call `Display` on each `ValidationIssue`. The new variants will be picked up automatically via the `Display` impl added in Task 1.

Verify this assumption by running the existing tests after Tasks 1 and 2. If `validate_json_has_separate_arrays` or `validate_with_warnings_flag_shows_warnings` break, investigate and fix.

**How to verify:**
```
cargo test cli_expanded_validate_test
cargo test cli_validate_test
```

---

### Task 4: Write tests for new validation rules

**ACs addressed:** AC1, AC2, AC3, AC4, AC6

**Files:**
- Modify: `tests/cli_expanded_validate_test.rs`

**What to implement:**

Add these tests using the existing `setup_with_chain` helper where possible and creating new setup functions where the chain structure is insufficient:

**`all_stories_accepted_warns_draft_rfc`** (AC1):
Setup: RFC draft, Story accepted (single story). Run `validate_full()`. Assert `AllChildrenAccepted` warning present with RFC as parent.

**`all_iterations_accepted_warns_draft_story`** (AC2):
Setup: RFC accepted, Story draft, Iteration accepted. Run `validate_full()`. Assert `AllChildrenAccepted` warning present with Story as parent.

**`partial_children_no_all_accepted_warning`** (AC4):
Setup: RFC draft, two stories -- one accepted, one draft. This requires a new setup function (`setup_with_two_stories`) since `setup_with_chain` only creates one story. Run `validate_full()`. Assert no `AllChildrenAccepted` warning for the RFC.

**`accepted_story_draft_rfc_orphaned`** (AC3):
Setup: RFC draft, Story accepted, Iteration accepted. Run `validate_full()`. Assert `UpwardOrphanedAcceptance` warning present with Story as path and RFC as parent.

**`all_children_accepted_json_output`** (AC6):
Setup: RFC draft, Story accepted. Run `run_json()`. Parse JSON. Assert the `warnings` array contains a string matching `"all children accepted"`.

**`setup_with_two_stories` helper:**
```rust
fn setup_with_two_stories(rfc_status: &str, story1_status: &str, story2_status: &str) -> (TempDir, Store) {
```
Creates RFC-001, STORY-001 (implements RFC-001), STORY-002 (implements RFC-001). No iteration.

**How to verify:**
```
cargo test cli_expanded_validate_test
```

## Test Plan

| Test | ACs | Properties traded | Notes |
|------|-----|-------------------|-------|
| `all_stories_accepted_warns_draft_rfc` | AC1 | Fast, Isolated, Specific | Unit test against store |
| `all_iterations_accepted_warns_draft_story` | AC2 | Fast, Isolated, Specific | Unit test against store |
| `partial_children_no_all_accepted_warning` | AC4 | Fast, Isolated, Specific | Negative case, needs two-story setup |
| `accepted_story_draft_rfc_orphaned` | AC3 | Fast, Isolated, Specific | Verifies generalised orphaned acceptance |
| `all_children_accepted_json_output` | AC6 | Fast, Readable | Round-trips through JSON serialisation |
| Existing `orphaned_acceptance_warning` | AC3 (regression) | - | Existing test must still pass unchanged |

All tests are unit-level, operating on in-memory `Store` instances with tempdir fixtures. No integration tests needed since the validation output path (`run_json`/`run_human`) is already covered by existing tests and the new variants flow through the same code path.

## Notes

- The `reverse_links` HashMap is already built during `Store::load()` (line 54-66 of store.rs). No new data structures needed, just new traversal logic in `validate_full()`.
- The existing `OrphanedAcceptance` variant stays unchanged. Adding `UpwardOrphanedAcceptance` as a separate variant avoids modifying the semantics of the existing check or breaking existing tests.
- Task 3 may be a no-op if the Display impl is sufficient. Included as a verification step.
