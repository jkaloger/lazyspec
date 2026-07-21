---
title: Catch up on comments since a revision
type: story
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As an agent or a returning reviewer, I want to see only the comments added since a known revision, so that I can catch up on what changed without re-reading a whole thread.

Adds the `since` capability to the `CommentStore` trait and exposes it on the read path.

## Scope

- `CommentStore::since(doc, cursor)` implemented for filesystem and git-ref stores.
- `lazyspec comments <doc> --since <rev>` — returns only comments after the cursor.
- `--json` returns the delta with a cursor the caller can persist for the next poll.

Out of scope: any scheduler / control loop (RFC non-goal — orchestration is a downstream consumer, not built here).

## Acceptance Criteria

- **Given** a thread with comments before and after revision `R`, **when** I run `comments <doc> --since R --json`, **then** only comments after `R` are returned.
- **Given** no comments since `R`, **then** an empty set is returned (not an error).
- **Given** the returned delta, **then** it carries a cursor usable as the `--since` argument for the following call.
- **Given** the git-ref store, **then** `--since` resolves the cursor against the ref namespace; against filesystem it resolves against commit/ULID ordering.

## Notes

This capability is what an external orchestrator's change-detection loop would consume — a legitimate downstream consumer, explicitly not an orchestrator built inside lazyspec.

