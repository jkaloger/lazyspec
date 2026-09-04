---
title: Remove acceptance-gating validation findings
type: iteration
status: complete
author: unknown
date: 2026-09-04
tags: []
related:
- implements: STORY-024
- related-to: STORY-022
---

## Objective

Drop the three acceptance-gating findings from `validate`. Child status no longer judged against parent status.

## Satisfies

Reverses STORY-022 AC3 and STORY-024 AC1-AC4, AC6. Those ACs describe findings this slice deletes; stories keep their text, iteration records the reversal.

## Context

- Root cause: `src/engine/validation.rs:494` compares parent status to literal `accepted`. Lifecycle is config (`.lazyspec.toml` `lifecycle`); `in-progress`/`complete` parents trip it. Upward rule at line 649 hardcodes `draft`/`review` same way. Decision: no gate, not a smarter gate.
- Stories: STORY-022 (origin of `OrphanedAcceptance`), STORY-024 (`UpwardOrphanedAcceptance`, `AllChildrenAccepted`).
- Touch: `src/engine/validation.rs`, `README.md` line ~716, tests in `tests/integration/{cli_expanded_validate_test,cli_init_test,validate_ignore_test,cli_fix_config_test}.rs`.

## Tasks

1. Delete enum variants `OrphanedAcceptance`, `AllChildrenAccepted`, `UpwardOrphanedAcceptance` and their `Display` arms.
2. Delete the `OrphanedAcceptance` push in `BrokenLinkRule` (line ~494). Keep `RejectedParent`, `SupersededParent`.
3. Delete `StatusConsistencyRule` struct, impl, registration (line ~1319). It emits nothing else.
4. Remove or retarget test assertions on the three variants in the four integration files. Keep tests that also cover surviving findings.
5. Rewrite README line ~716 to name only `implements rejected document` and `implements superseded document`.
6. Update the rustdoc at `src/engine/config.rs:1447` that names "the two findings that walk" `child_types_for`. Keep `child_types_for`; `src/engine/prompt.rs:177` still calls it.

## Out of scope

- Lifecycle-aware replacement gate. Deliberately not built.
- Marking STORY-022 / STORY-024 superseded. Human call.
- Other hardcoded status names (`rejected`, `superseded`, `accepted` in sync/issue_body).

## Principles

- Convention principle 1, 6. Dictums via `lazyspec convention`.
- Engine change: check TUI and web view for any consumer of the removed variants.

## Verification

`cargo run -- validate --json` on this repo: `warnings` contains no `parent not accepted` strings. `cargo test` green. `cargo build` no dead-code warnings.
