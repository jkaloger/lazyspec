---
title: Stamp reviewed with HEAD when pinning
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-270
---

## Objective

`pin <id>` writes `reviewed: <HEAD sha>` alongside its existing `@ref` blob-hash pinning, in one run.

## Satisfies

STORY-270 AC1, AC2, AC3, AC4, AC5. Closes the story.

## Context

- Story + ACs: STORY-270
- One verb, no `review` alias; `head` on `GitRefOps` rather than a second trait: RFC-068 §Decisions 6, §Interfaces
- Touch:
  - `src/engine/git_ref.rs:11` `GitRefOps` -- add `head`. `read_commit_timestamp` (trait `:48`, real impl `:308`, fake `:353` and `:448`) is the precedent for a read-only query on this trait. Follow it down to the fake's queued-result shape.
  - `src/cli/pin.rs:40` `pin_document` returns a rewritten body. `reviewed` is frontmatter, not body -- write it through the same frontmatter writer `update` and `src/engine/provenance.rs:33` use. Do not string-edit the front matter.
  - `PinResult` (`src/cli/pin.rs:26`) gains the sha, so `--json` (AC4) reads a field rather than re-deriving it.
- AC5: read `HEAD` before writing anything. A failure leaves the document byte-identical.
- AC2 is plain overwrite. There is no history to keep on `reviewed`.

## Tasks

1. Add `head` to `GitRefOps`, its real implementation, and a fake returning a fixed sha, mirroring `read_commit_timestamp`.
2. Test-first: `pin` on a document with no `reviewed` writes the fake's sha; on one with a stale value, replaces it (AC1, AC2).
3. Test-first: a document with `@ref` directives gets blob hashes and `reviewed` in the same run (AC3).
4. Test-first: `head` erroring makes the command report it and leaves the file unchanged (AC5).
5. Implement; carry the sha on `PinResult` and into `--json` (AC4).
6. README: the `pin` description.

## Out of scope

- Judging `reviewed` -- RFC-069.
- Using it for rename detection -- STORY-271.
- A `review <id>` alias. RFC-068 §Decisions 6 rejects it.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-002: the trait exists for the testability seam, and the fake lives in the consumer's `#[cfg(test)]`.

## Verification

`cargo run --quiet -- pin RFC-068 --json | jq -r .reviewed` equals `git rev-parse HEAD`, and the file's `@ref` hashes are unchanged from a prior `pin`. Revert the document afterwards.
