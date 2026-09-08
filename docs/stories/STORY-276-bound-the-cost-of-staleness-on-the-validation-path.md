---
title: Bound the cost of staleness on the validation path
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: STORY-274
- implements: RFC-069
---

As a maintainer running `status --json` or moving the cursor in the TUI, I want the staleness rule to cost a bounded number of git subprocesses, so that stamping `reviewed` on documents does not make routine commands and the TUI validation refresh slow in proportion to the size of the docs tree.

## Acceptance criteria

- Given a docs tree where every document carries `reviewed`, when I run `status --json`, then the command issues a bounded number of git subprocesses rather than one `git cat-file -p` per document plus one `git diff --numstat` per pinned document.
- Given the same tree, when the TUI refreshes validation, then the render loop does not block on staleness computation.
- Given a document whose `reviewed` sha and whose `HEAD` are both unchanged since the last computation, when staleness is computed again, then no new git subprocess is issued for it.
- Given an `age`-driven document whose own `date` is more recent than the `aging` threshold, when `StaleRule` runs, then it issues no git subprocess for that document: no anchor can make it stale.
- Given `why <path> --json` over a path governed by N documents, when it runs, then it issues N git subprocesses rather than 2N — `compute` fetches `read_commit_timestamp` for an `age_days` that `drifted` discards.

## Notes

Filed out of ITERATION-425's review. `StaleRule` runs inside `validate_full`, which has two non-`validate` consumers: `src/cli/status.rs` (`status --json`) and `src/tui/state/app.rs` `refresh_validation`, reached from eleven event-loop call sites.

The cost is zero today only because no document in this repo carries a `reviewed` anchor, and `compute` shells out only when `reviewed.is_some()`. STORY-274 stamps `reviewed` on every local status transition, so the anchors start appearing as soon as it lands. Measured shape at that point: roughly 1750 subprocesses across 877 documents, synchronously, per invocation.

RFC-069 Decision 6 rejected exactly this shape for the TUI badges and put them in a background worker; the validation path has no equivalent guard. RFC-069 also names caching a non-goal, with a known key of `(reviewed, HEAD)` — Decision 2 rejected it for want of a measured need. This is the measured need, and RFC-069 has been amended to say so.

The last two criteria came out of the batch's comprehensive review and are cheaper than the cache: the age short-circuit needs no new state at all and cuts most of the 1750 calls on its own, since most documents are age-driven and most are young. Try both before reaching for the key.
