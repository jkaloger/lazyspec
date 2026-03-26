---
title: Forward and Backward Context with Related Records
type: iteration
status: accepted
author: agent
date: 2026-03-10
tags: []
related:
- implements: STORY-054
---




## Changes

### Task 1: Extend `resolve_chain` to return the target index and build forward context

**ACs addressed:** Forward context, "You are here" marker

**Files:**
- Modify: `src/cli/context.rs`

**What to implement:**

`resolve_chain` currently returns `Vec<&DocMeta>` -- just the backward chain. It needs to also return:
1. The index of the target document within the chain (so callers know which card to mark)
2. The forward children of the target document (documents whose `implements` points at the target)

Change `resolve_chain` to return a struct:

@ref src/cli/context.rs#ResolvedContext@c4c8fc626552b93f404aa57ba9ae9442b9c307ab

After building the backward chain (existing logic), walk forward from the target: use `store.reverse_links` or iterate all docs to find documents that `implements` the target. These are `forward` entries.

For `related`: iterate all documents in the chain, collect their `RelatedTo` links, resolve them via `store.get()`, deduplicate by path.

**How to verify:**
`cargo test` -- new tests in Task 4 will cover this.

---

### Task 2: Update `run_human` for forward context, "you are here" marker, and related section

**ACs addressed:** Forward context (CLI), "You are here" marker, Related records (CLI)

**Files:**
- Modify: `src/cli/context.rs` (`run_human`, `mini_card`)

**What to implement:**

1. Add a `marker: bool` parameter to `mini_card`. When true, append `  ← you are here` after the closing `╯` on the top-right line (the `╮` line). In no-color mode, append after `+`.

2. In `run_human`, use the new `ResolvedContext` from Task 1. For each card in the chain, pass `marker: i == ctx.target_index`.

3. After the last card in the chain, if `ctx.forward` is non-empty, render the forward children with `├─`/`└─` tree connectors (same style as the existing children rendering on lines 92-104, but for the target's forward children specifically). The existing children_of rendering for ancestor nodes in the chain stays as-is.

4. After the chain + forward section, if `ctx.related` is non-empty, render a `─── related ───` header (use `dim()` for styling), then list each related doc as `  SHORTHAND  Title [status]` where SHORTHAND is extracted from the path (e.g. `RFC-002`), Title is `doc.title`, and status uses `styled_status()`.

5. If `ctx.related` is empty, omit the related section entirely.

**How to verify:**
`cargo test` -- existing `context_human_output` test should still pass. New tests in Task 4 cover the extended output.

---

### Task 3: Update `run_json` to include `related` array

**ACs addressed:** JSON output

**Files:**
- Modify: `src/cli/context.rs` (`run_json`)

**What to implement:**

Use `ResolvedContext` from Task 1. The JSON output gains a `related` key alongside `chain`:

```json
{
  "chain": [...],
  "related": [
    { "path": "...", "title": "...", "type": "...", "status": "...", ... }
  ]
}
```

Each related document uses `doc_to_json_with_family` (same as chain entries). The `chain` field continues to include forward children via the existing `children` key on each chain entry. The `related` array is the deduplicated set of `RelatedTo` targets from all chain documents. If empty, emit `"related": []`.

**How to verify:**
`cargo test` -- new tests in Task 4.

---

### Task 4: Tests for CLI context changes

**ACs addressed:** All CLI ACs

**Files:**
- Modify: `tests/cli_context_test.rs`

**What to implement:**

Add test fixtures and cases:

1. **Forward context from RFC:** Create an RFC with two Stories implementing it. Run `resolve_chain` from the RFC. Assert `forward` contains both Stories.

2. **Forward context from Story:** Create RFC -> Story -> 2 Iterations. Run from the Story. Assert chain has [RFC, Story], forward has both Iterations.

3. **"You are here" marker:** Run `run_human` from a Story in a 3-level chain. Assert output contains `← you are here` on the Story card line, not on RFC or Iteration cards.

4. **Related records in human output:** Add `related to` links to the RFC. Run `run_human`. Assert output contains `─── related ───` and the related document titles.

5. **Related records omitted when none:** Run `run_human` on a chain with no `related to` links. Assert output does not contain `related`.

6. **JSON related field:** Run `run_json` on a chain where the RFC has `related to` links. Parse JSON, assert `related` array is non-empty and contains the expected documents.

7. **JSON related empty:** Run `run_json` on a chain with no related links. Assert `related` is an empty array.

8. **No forward children:** Run from an Iteration (leaf). Assert `forward` is empty, output matches existing behavior.

**How to verify:**
`cargo test --test cli_context_test`

---

### Task 5: TUI relations tab shows chain, children, and related records

**ACs addressed:** TUI context view

**Files:**
- Modify: `src/tui/ui.rs` (`draw_relations_content`)
- Modify: `src/tui/app.rs` (`relation_count`, `navigate_to_relation`)

**What to implement:**

Restructure `draw_relations_content` to show three sections instead of grouping by relationship type:

1. **Chain** section (dim italic header `  chain`): show the backward chain from the selected doc. For each ancestor, render as `    Title  type [status]` (same format as existing relation items). The chain walks `implements` upward, same as `resolve_chain`.

2. **Children** section (dim italic header `  children`): show documents that implement the selected doc (reverse `implements` lookup). Same item format.

3. **Related** section (dim italic header `  related`): show `RelatedTo` links from the selected doc. Same item format as today.

The flat indexing for selection (`selected_relation`, `flat_index`) stays the same pattern but now indexes across all three sections. Update `relation_count` to return the total across all sections. Update `navigate_to_relation` to resolve the correct target path from the new section layout -- it needs to map `selected_relation` to the right document across chain, children, and related items.

Navigation (Enter key) already calls `navigate_to_relation` which jumps to the target doc. This works as long as the path mapping is correct.

**How to verify:**
`cargo run` then launch the TUI, select a document with relations, switch to Relations tab. Verify chain, children, and related sections appear. Press Enter on a relation to navigate.

## Test Plan

| # | AC | Test | Properties |
|---|---|---|---|
| 1 | Forward context (RFC) | Fixture: RFC + 2 Stories. `resolve_chain(RFC)` -> forward contains both Stories | Isolated, Deterministic, Specific |
| 2 | Forward context (Story) | Fixture: RFC -> Story -> 2 Iters. `resolve_chain(Story)` -> chain=[RFC,Story], forward=[Iter1,Iter2] | Isolated, Deterministic |
| 3 | Forward context (leaf) | `resolve_chain(Iteration)` -> forward is empty | Isolated, Behavioral |
| 4 | You are here marker | `run_human(Story)` in 3-level chain -> output contains `← you are here` on Story line only | Behavioral, Specific |
| 5 | Related records (human) | RFC with `related to` links. `run_human` -> `─── related ───` + titles | Behavioral, Readable |
| 6 | Related records omitted | No related links -> no `related` in output | Behavioral |
| 7 | JSON related (present) | `run_json` -> `related` array with expected docs | Isolated, Specific |
| 8 | JSON related (empty) | `run_json` -> `"related": []` | Isolated |

> [!NOTE]
> TUI changes (Task 5) are verified manually. The existing `navigate_to_relation` tests in the codebase (if any) may need updating to account for the new section layout.

## Notes

The existing `children_of` rendering in `run_human` (lines 92-104) shows child documents (subfolder children) for each node in the chain. This is distinct from "forward context" which shows documents that `implements` the target. Both should appear: subfolder children inline with their parent card, forward implementers after the target card.
