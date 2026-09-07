---
title: Stamp a document as reviewed against current code
type: story
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
- blocks: STORY-271
---

As a document author who has just checked a document against the code it governs, I want `pin <id>` to record the current commit in `reviewed`, so that one verb captures the review and later tooling has an anchor to diff from.

## Acceptance criteria

- Given a document, when I run `pin <id>`, then its frontmatter gains `reviewed: <HEAD sha>` and `show <id>` prints it.
- Given a document that already has `reviewed`, when I run `pin <id>`, then the value is replaced with the current `HEAD`.
- Given a document with `@ref` directives, when I run `pin <id>`, then blob hashes are pinned exactly as before, in the same run.
- Given `pin <id> --json`, when it succeeds, then the output includes the `reviewed` sha written.
- Given a repo with no git history or a detached state where `HEAD` cannot be read, when I run `pin <id>`, then the command reports the failure and leaves the document unchanged.

## Notes

Extends the existing verb; no `review <id>` alias, per RFC-068 decision 6. `HEAD` is read through `GitRefOps::head`, so the fake in tests returns a fixed sha.
