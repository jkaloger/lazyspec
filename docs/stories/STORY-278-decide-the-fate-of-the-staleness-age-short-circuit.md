---
title: Decide the fate of the staleness age short-circuit
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
---

As a maintainer, I want one answer to whether an age short-circuit belongs in `StaleRule` at all, so that `show` and `validate` stop disagreeing about whether a document is stale.

## Acceptance criteria

- Given a document dated today whose `reviewed` anchor points at a 200-day-old HEAD — a dormant governs root, most exposed on a docs-repo split — when I run `show <id>` and when `StaleRule` runs over it, then the two surfaces agree on its band. Today `show` bands it `stale` and `StaleRule` skips it, because `cannot_be_stale` assumes an anchor is never older than the document's `date`.
- Given a drift-driven document with nothing pinned, when `cannot_be_stale` runs, then it short-circuits or is documented as unable to. `compute` falls back to `Age` when there is nothing to diff, so `terms.driver` reads `Drift` and the document pays a `read_commit_timestamp` the short-circuit was meant to save.
- Given the decision is to delete the short-circuit, when it is removed, then `status --json` over a fully-stamped tree issues no more subprocesses on a warm memo than it does today, and STORY-276 AC1 and AC4 are amended to say why the memo alone suffices.

## Notes

Filed out of the RFC-069 chunk's comprehensive review (STORY-276, STORY-277). Three findings that are one decision, not three fixes.

STORY-276 AC4 asserts the false premise as fact, so the code conforms to its story and the hole is in the acceptance criterion. That is why this is a story rather than a bug.

The reviewer's own read: `cannot_be_stale` is now largely subsumed by the cache that shipped alongside it — the `timestamps` memo keys on the sha and so answers forever — and the short-circuit's only remaining purchase is the cold-memo case, bought at the price of the divergence above. STORY-276's Notes said "try both before reaching for the key", not "keep both once you have it". Deleting it is the smaller codebase; measure the cold path before deciding.
