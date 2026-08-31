---
title: Reject an edge edit in the loader's own words
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
---

## Objective

An edge edit the loader would refuse -- unknown type, unknown relationship, contradictory traversal, `required` on a wildcard `from` -- is refused with `Config::parse`'s own message and nothing else, and the panel lands the cursor on the edge field that caused it.

## Satisfies

STORY-260 AC4. AC1, AC2, AC5 landed in ITERATION-386, ITERATION-387 and ITERATION-388; AC6 lands in ITERATION-390, AC3 in ITERATION-391.

## Context

- Story + ACs: STORY-260, and §Notes: "two spellings of the same error is how they drift"
- Where each error AC4 names actually lives: unknown type at `src/engine/config.rs:1327-1336` and unknown relationship at `:1337-1345` (both landed); `required` on a wildcard `from` in ITERATION-370; a traversal-role disagreement between overlapping rows in ITERATION-372. AC4 names errors from three separate slices, which is why two of them are blocking edges here
- Touch:
  - `src/tui/state/app.rs:1621-1626` -- the re-parse and `settings_footer_error`
  - `src/tui/state/app.rs:1642-1702` `settings_jump_to_violation` and `:1723-1740` `settings_jump_to_field`
  - `src/engine/config.rs:1273-1400` `parse_inner`, only if the seam below is taken
- **The seam for the message already exists.** `settings_commit_write` re-parses the exact bytes destined for disk through `Config::parse` and puts `e.to_string()` straight into `settings_footer_error`. That is the loader's text verbatim, which makes AC4 a wiring slice rather than a validation slice. Nothing in this slice should format an edge error. The reason it only became reachable now is ITERATION-388: before edges were written, the re-parsed bytes did not contain the offending edit, so the guard passed and the edit was silently dropped.
- **The seam that does not exist.** AC4 says "when the designer commits it", and the panel has two commits: the field commit (`settings_confirm_edit`, `app.rs:1330-1372`) and the save. Only the save reaches `Config::parse`, because `Config::parse` takes a `&str` and the invariant checks are interleaved with the raw-to-typed conversion inside `parse_inner` -- there is no `fn check(&self) -> Result<()>` on `Config` that a live buffer could be handed to. Rejecting at field-commit time therefore needs an engine change: lift `parse_inner`'s post-conversion invariant block into a `Config`-level check that `parse_inner` itself calls, so the early rejection is the same code and not a copy. **Decide which reading of "commits" this slice implements and record it.** Save-time is free and already correct. Edit-time reads better and costs the extraction plus a judgement about which invariants are even expressible on a `Config` -- the missing-`[[types]]` and missing-`[[relationships]]` bails (`config.rs:1277-1290`) are not, since they are about absent raw sections. If save-time is chosen, amend the story so the AC stops implying otherwise.
- **Where a second spelling can still creep in.** `settings_jump_to_violation` finds the offending field by inspecting the buffer in `Config::parse`'s order -- deliberately, "more reliable than string-matching the error" (its doc comment). Its three branches cover sqids, github, and a best-effort landing on category 4. Edge violations need arms, and every arm is a second implementation of a loader predicate: the *message* is shared, the *attribution* is not. That is STORY-260 §Notes' drift, relocated rather than removed. Keep each arm to a predicate the engine already exposes -- `EdgeDef::matches` (`config.rs:67-78`) and the selectors' `names()` accessor -- rather than re-deriving a check, and test the landing per arm so a divergence fails loudly.
- After ITERATION-384, `Config::parse` also fails on a surviving `[[rules]]` block -- an error the Edges panel did not cause and can show no field for. The fallback must not attribute it to an edge field. That is the blocking edge to 384.
- `settings_jump_to_field` takes a hardcoded category index; every existing caller passes `1` or `4`. Edge arms pass `3`. Those literals are the same silent-index hazard ITERATION-386 Task 3 re-based tests for -- resolve them against `settings_categories()` rather than adding a fourth magic number, or say why not.

## Tasks

1. Test-first through the App: set `to` to a type name absent from `[[types]]`, save, and assert `settings_footer_error` equals what `Config::parse` produces for the same bytes -- compared against `Config::parse(...).unwrap_err().to_string()`, never against a literal. A literal in the test is the second spelling the AC forbids, written into the guard that is supposed to prevent it.
2. The same shape for the other three: an unknown relationship in `via`, `required` set on a `from = "*"` row, and two rows disagreeing on `traversal`.
3. Assert nothing is written on rejection: file bytes unchanged, `settings_dirty` still true, the buffer still holding the edit. `settings_commit_write` already guarantees this; pin it for the edge path.
4. Add edge arms to `settings_jump_to_violation` in the loader's check order, each landing on the specific `FieldPath::Edge` key at fault, with a test per arm.
5. Resolve the "commits" question from Context, and put the answer in the doc comment of whichever function ends up owning the check.
6. If the engine seam is taken: extract the invariant block from `parse_inner` and have `parse_inner` call it, so the existing load tests (`config.rs:1988-2050`) cover the extracted function unchanged. If it is not taken, say so in the same place, so the next reader does not re-litigate it.

## Out of scope

- Adding any load-time check. All four errors AC4 names are the loader's, and three of them are other iterations' work; this slice surfaces them and adds none.
- Enforcing unique edge names -- the hole ITERATION-388 recorded. It produces no load error, so there is no message to surface and AC4 does not reach it.
- Refusing an empty target set. ITERATION-387 closed that in the panel precisely because the loader has no error for it; that asymmetry stays until someone gives the loader the check.
- Seeding a valid row (AC6) -> ITERATION-390, which asserts its seed against whatever check this slice settles on.
- The target-set picker (AC3) -> ITERATION-391, whose commit path must route through the same rejection.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: validation belongs to the engine's load path and the TUI's job is to show its answer. Convention §"CLI Patterns / Output & Errors": `anyhow` context messages are written for the person reading them, which is exactly why the panel must not rewrite them.

## Verification

In the TUI, retype an edge's `to` to `nonsense` and press `w`: the footer reads `edge "..." names unknown type "nonsense" (not declared in [[types]])`, the cursor lands on that edge's `to` row, and `.lazyspec.toml` is untouched.
