---
title: Reply to a comment and see the thread nest
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer, I want to reply to an existing comment and see the thread nest, so that a discussion stays structured instead of flat.

Layers threading onto the walking skeleton: the `in_reply_to` envelope field plus the engine fold that builds a reply tree from the append-only stream.

## Scope

- `lazyspec comment add <doc> --reply-to <comment-id>` — post a comment parented to another.
- Engine fold builds a tree from `in_reply_to`: root comments (null parent) and nested replies.
- `lazyspec comments <doc>` renders the tree: `--json` as nested structure, pretty print as indentation.

Out of scope: resolution state (later story), other stores.

## Acceptance Criteria

- **Given** a root comment `C1`, **when** I run `lazyspec comment add <doc> --reply-to C1 --body "reply"`, **then** the new comment persists with `in_reply_to = C1`.
- **Given** a root comment with two replies, **when** I run `lazyspec comments <doc> --json`, **then** the replies appear nested under the root in the folded tree, ordered by ULID.
- **Given** the same thread, **when** I run `lazyspec comments <doc>` (pretty), **then** replies are shown indented beneath their parent.
- **Given** `--reply-to` pointing at a non-existent comment id, **then** the command fails with a clear error (`--json` diagnostic).
- **Given** clock drift between two writers, **then** causal order via `in_reply_to` is authoritative over ULID wall-clock order.

## Notes

Thread structure is derived, never stored as a mutable field — it folds from the envelope on every read.

