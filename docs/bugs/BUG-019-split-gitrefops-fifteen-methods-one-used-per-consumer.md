---
title: 'Split GitRefOps: fifteen methods, one used per consumer'
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
---

Found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`).

`GitRefOps` is now 15 methods. `pin` uses one of them (`head`); `GovernsNoMatchRule` uses one (`renames`). The cost is already visible: `tests/integration/cli_fix_governs_test.rs` hand-writes 14 `unreachable!()` stubs — 90 of its 320 lines — purely to supply `renames`.

Two dictums point at this:

- DICTUM-002: "A consumer should never need to implement methods it doesn't use."
- DICTUM-004: "If a test requires 50 lines of setup to test 1 line of behavior, the API is wrong. Fix the API, don't write the 50-line test."

RFC-068 §Interfaces deferred the split under principle 6 ("add indirection when there are two concrete uses, not before"). The batch has now reached two concrete uses, so the deferral has expired. Principle 6 permits the split; DICTUM-002 prefers it.

Related, same root cause: `MockGitRefClient` sits beside the trait in `src/engine/git_ref.rs::test_support` rather than in `#[cfg(test)]` of the consuming module, against DICTUM-002. Because `test_support` is `#[cfg(test)]`, integration tests cannot reach it — which is precisely why the duplicate stub above exists. Two tests there (`git_ref.rs:870`, `:894`) assert on the fake rather than on production code, against DICTUM-004's predictive test rule.

A `GitQuery { head, renames, read_commit_timestamp }` split was the reviewer's suggestion.
