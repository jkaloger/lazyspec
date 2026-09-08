---
title: See how stale a document is
type: story
status: in-progress
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
- blocks: STORY-273
- blocks: STORY-274
- blocks: STORY-275
---

As an agent reading a document before I trust it, I want `show` to tell me how stale it is and `why` to flag drift per governing document, so that I can tell a spec reviewed against current code from one describing code that has been rewritten twice.

## Acceptance criteria

- Given a type with `staleness = "drift"`, a document with `reviewed` and `governs` set, and commits touching a governed path since `reviewed`, when I run `show <id> --json`, then `staleness.band` is `stale`, `staleness.driver` is `drift`, and `staleness.drift` carries the file, insertion and deletion counts from `git diff --stat`.
- Given the same document with no commits touching a governed path since `reviewed`, when I run `show <id> --json`, then `staleness.band` is `fresh`.
- Given a type with `staleness = "age"` (the default) and `[staleness] aging = "90d"`, `stale = "180d"`, when I run `show <id> --json` for documents anchored 10, 100 and 200 days ago, then the bands are `fresh`, `aging` and `stale` respectively.
- Given a `drift` type document with no `governs`, or with `governs` but no `reviewed`, when I run `show <id> --json`, then `staleness.driver` is `age`, the band comes from the age thresholds, and `staleness.drift` is zero.
- Given a document with `reviewed` set, when staleness is computed, then `staleness.anchor` is that sha and `age_days` is measured from the anchor commit's timestamp; given no `reviewed`, then `anchor` is the document's `date` and `age_days` is measured from it.
- Given any document, when I run `show <id>` without `--json`, then one line reads `staleness: <band> (<driver>, <n> files since <sha>, <n>d)`.
- Given a path governed by a document whose governed files have changed since its `reviewed`, when I run `why <path> --json`, then that record carries `drifted: true`; given no change since `reviewed`, or no `reviewed`, then `drifted` is `false`.
- Given a command that neither reports a band nor runs full validation — `list`, `create`, `link` and their peers — when it runs, then no staleness computation is issued for it: the band is computed only by `show`, by `why`, by the TUI's background badge worker on selection, and by the `validate_full` path that `validate`, `status --json` and the TUI validation refresh share.

## Notes

Walking skeleton for RFC-069. Lands `[staleness]` config (`aging`, `stale`), the per-type `staleness` key defaulting to `age`, `engine::staleness::compute`, `GitRefOps::diff_stat`, and the `Staleness` object on the `show` and `why` read surfaces. Spec, convention and dictum become `drift` types; RFC and ADR stay `age`.

Computed on demand only, never on `DocMeta` and never at store load, so a command off the read and validation paths pays nothing. No caching (named non-goal). Validation finding, review stamping, and the TUI and web badges are later slices.
