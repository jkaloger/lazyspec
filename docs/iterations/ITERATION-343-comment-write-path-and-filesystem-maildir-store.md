---
title: Comment write path and filesystem maildir store
type: iteration
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: STORY-232
---

## Objective

Post a comment to a document, persisted as an immutable ULID-named markdown file under the document's `comments/` directory.

## Satisfies

STORY-232 AC1 (create + file), AC2 (stdin body), AC5 (non-existent doc error), AC6 (empty/missing body error). AC3, AC4 (listing + fold) deferred — see Out of scope.

## Context

- Story + ACs: STORY-232 (walking skeleton — do not restate).
- Envelope shape + maildir layout: RFC-060 §Design ("The comment as a blackboard posting", "Filesystem impl — comment-per-file").
- Conventions: `docs/convention/DICTUM-002-trait-usage.md` (no trait for a single impl — principle 6), `docs/convention/DICTUM-004-testing.md` (TDD), `docs/convention/DICTUM-006-cli-patterns.md` (`--json`).
- Touch:
  - `src/engine/comment.rs` (new) — `Comment` envelope (`id`, `author`, `created_at`, `in_reply_to`, `refs`, opaque `attributes` map) + fs maildir append. No `CommentStore` trait yet (single impl; extracted at STORY-237).
  - `src/engine/document.rs` — resolve a doc's `comments/` directory from its path.
  - `src/engine.rs` — register the `comment` module.
  - `src/cli/comment.rs` (new) — `comment add` handler.
  - `src/cli.rs` — add `Comment` subcommand to the `Commands` enum.

## Tasks

1. Test-first: engine tests for append — file created under `comments/<ulid>.md`, envelope fields correct (`in_reply_to = None`), and appended files are never mutated. Cover the non-existent-doc and empty-body error cases.
2. Implement `Comment` envelope + ULID id + fs maildir append in `src/engine/comment.rs`.
3. Wire `comment add <doc> --body <..> | --body-file -` (stdin via `-`) with `--json`, mirroring existing CLI handler shape (see `src/cli/create.rs`).
4. Error paths: unknown doc id → error + `--json` diagnostic, no write; neither `--body` nor `--body-file` / empty body → missing-body error, no write.

## Out of scope

- Listing / ordering fold / pretty output → ITER-B (STORY-232 AC3, AC4).
- Threading (`in_reply_to` population), attribute schema/validation, `CommentStore` trait, git-ref/remote stores, `--since`, TUI/web → later STORY-233…243.
- `refs` is carried on the envelope but left unpopulated (no CLI sets it here).

## Principles/conventions

Pointers only: `docs/convention/CONVENTION.md` and DICTUM-002/004/006 above. ULID for coordination-free ordering (RFC-060 ADR).

## Verification

`lazyspec comment add RFC-060 --body "note"` creates one file under `docs/rfcs/RFC-060.../comments/`; `comment add NOPE-999 --body x` exits non-zero writing nothing; `comment add RFC-060` with no body exits non-zero writing nothing.

