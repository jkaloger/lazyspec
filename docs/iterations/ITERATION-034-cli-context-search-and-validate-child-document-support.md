---
title: CLI context, search, and validate child document support
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-041-cli-child-document-support.md
---



## Changes

### Task 1: Update `context` command to include children as relationships

**ACs addressed:** AC3

**Files:**
- Modify: `src/cli/context.rs`

**What to implement:**

The `context` command currently builds a chain by following `implements` links upward via `resolve_chain()`. After displaying the chain, it should also show children for each document in the chain that has them.

Update `run_human()`:
- After rendering each `mini_card` in the chain, check `store.children_of(&doc.path)`. If non-empty, render child documents beneath the card with indentation, before the chain connector to the next item. Use a simple format: `  ├─ {child_title}  ({child_path})` for each child, with `└─` for the last one.

Update `run_json()`:
- Use `doc_to_json_with_family(d, store)` (from ITERATION-033 Task 1) instead of `doc_to_json(d)` so each chain item includes its `children` array in the JSON output.

Both `run_human` and `run_json` already receive `&Store`, so no signature changes needed.

**How to verify:**
`cargo test` -- tests in Task 4. Manual: `cargo run -- context RFC-003` where RFC-003 is a parent with children.

---

### Task 2: Verify search matches child content independently

**ACs addressed:** AC5

**Files:**
- Modify: `src/cli/search.rs` (potentially no code changes needed)

**What to implement:**

The `store.search()` method already iterates all `store.docs.values()`, which includes child documents since ITERATION-032. Search should already match child documents independently since each child is a full document in the store.

Verify this is the case. If search already works correctly for children, this task is just confirming existing behavior with tests (Task 4). No code changes expected.

If for some reason children are filtered or excluded, add them back. The search result should include the child document's path (which includes the parent folder), making it clear which child matched.

**How to verify:**
`cargo test` -- specific test in Task 4 that creates a child with unique content and searches for it.

---

### Task 3: Verify validate checks children independently

**ACs addressed:** AC6

**Files:**
- Modify: `src/engine/validation.rs` (potentially no code changes needed)
- Modify: `src/cli/validate.rs` (potentially no code changes needed)

**What to implement:**

The `validate_full()` function iterates all `store.docs`, which includes children since ITERATION-032. Each child has its own frontmatter and is validated independently. Validation errors already reference the document path, which for children includes the parent folder (e.g. `docs/rfcs/RFC-003-multi/appendix.md`).

Verify this works correctly. The key scenarios to confirm:
1. A child with invalid frontmatter produces a validation error referencing the child's path specifically
2. A child with a broken link produces a broken link error referencing the child
3. A parent with valid frontmatter is not affected by a child's validation errors

If children are somehow skipped during validation (e.g. filtered by `validate_ignore` or missing from the docs map), fix the gap. Based on code review, this should already work.

**How to verify:**
`cargo test` -- specific test in Task 4 that creates a child with a broken link and validates.

---

### Task 4: Integration tests for context, search, and validate with children

**ACs addressed:** AC3, AC5, AC6

**Files:**
- Create: `tests/cli_child_context_test.rs`

**What to implement:**

Tests using `TestFixture` to set up parent-child structures and verify CLI behavior.

Planned tests:

1. **`context_includes_children_human`** (AC3): Create a parent RFC with children and a Story that implements it. Call `context::run_human()` for the Story. Assert the output includes child titles beneath the RFC card in the chain.

2. **`context_includes_children_json`** (AC3): Same setup. Call `context::run_json()`. Parse JSON. Assert each chain item that has children includes a `children` array.

3. **`search_matches_child_independently`** (AC5): Create a parent RFC with body "overview" and a child with body containing "unique-child-term". Search for "unique-child-term". Assert only the child is returned, not the parent.

4. **`search_does_not_include_parent_for_child_match`** (AC5): Same setup. Assert parent and siblings are absent from results when only the child matches.

5. **`validate_reports_child_errors_specifically`** (AC6): Create a parent with valid frontmatter and a child with a broken `related` link. Run `store.validate_full()`. Assert the error references the child's path, not the parent's.

6. **`validate_parent_unaffected_by_child_error`** (AC6): Same setup. Assert no errors reference the parent path.

Tradeoffs: Tests 3-4 (search) and 5-6 (validate) may confirm existing behavior rather than new code. This is intentional -- AC5 and AC6 require proof that children are handled correctly, and tests document the contract even when the implementation was free.

## Test Plan

| Test | AC | Property focus | Approach |
|------|-----|---------------|----------|
| context_includes_children_human | AC3 | Behavioral, Predictive | Assert child titles appear in chain output |
| context_includes_children_json | AC3 | Specific, Deterministic | Parse JSON, check `children` arrays in chain |
| search_matches_child_independently | AC5 | Behavioral, Specific | Search for child-only term, assert only child returned |
| search_does_not_include_parent_for_child_match | AC5 | Behavioral | Assert parent absent from child-only search |
| validate_reports_child_errors_specifically | AC6 | Specific, Predictive | Assert error path references child, not parent |
| validate_parent_unaffected_by_child_error | AC6 | Specific | Assert no errors on parent path |

## Notes

- Tasks 2 and 3 are likely zero-code-change tasks. The engine already treats children as full documents in the store, so search and validate should handle them automatically. The value of this iteration for those ACs is in the tests that prove the behavior.
- Task 1 (context) depends on `doc_to_json_with_family` from ITERATION-033. Build ITERATION-033 first.
- Build order: Task 1 -> Tasks 2 & 3 (parallel, likely verification only) -> Task 4.
