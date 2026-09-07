---
title: Repair a pin after a module rename
type: story
status: in-progress
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
---

As a document author whose module was renamed, I want the zero-match finding to name where the files went and `fix --governs` to rewrite the glob, so that repairing a stale pin is one command and a reviewable diff.

## Acceptance criteria

- Given a document with `reviewed` set and a glob that now matches nothing, when I run `validate --json`, then the finding's `renamed` lists `{from, to}` pairs from `git diff -M --name-status <reviewed>..HEAD` filtered to paths the old glob matched.
- Given those renames, when the finding is built, then `suggested_glob` is the longest common directory prefix of the `to` paths with `/**` appended.
- Given a document without `reviewed`, when its glob matches nothing, then `renamed` is empty and `suggested_glob` is null.
- Given renamed files spread across two directories, when the suggestion is built, then it is their common ancestor plus `/**`, and the finding still reports every pair so the author can narrow it by hand.
- Given findings with a `suggested_glob`, when I run `fix --governs`, then each such glob is rewritten in place and `reviewed` is left unchanged.
- Given `fix --governs --json`, when it runs, then the output lists each document and the old and new glob.
- Given a repaired document, when I run `validate` again, then no `governs-no-match` finding remains for that glob.

## Notes

Depends on the zero-match finding and on `reviewed` being stamped by `pin`. Renames come through `GitRefOps::renames`. The suggestion is a heuristic that over-widens on a module split; accepted in RFC-068 §Risks because `reviewed` stays put and RFC-069 will still flag drift.
