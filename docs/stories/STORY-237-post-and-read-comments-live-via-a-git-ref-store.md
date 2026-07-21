---
title: Post and read comments live via a git-ref store
type: story
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As an agent (or a human working fast), I want to post and read comments through a low-latency local git-ref store, so that rapid chatter is conflict-free and lock-free without committing a file per note.

This is the second concrete comment store, and it is what justifies extracting the `CommentStore` trait (convention principle 6: indirection when there are two concrete uses). Reuses the reservation-ref / lease machinery already in the codebase.

## Scope

- `CommentStore` trait extracted (`append`, `thread`, `since`) with the filesystem impl refactored behind it and a new git-ref impl added.
- Git-ref impl: body → `git hash-object -w` (immutable blob); pointer → `refs/lazyspec/comments/<doc>/<ulid>`. `git update-ref` old-value check as atomic compare-and-swap for optimistic concurrency.
- `comment_store` config: project default plus optional per-type override, values `filesystem | git-ref` (remote adapters are their own stories).
- `comment add` / `comments` work unchanged against whichever store is configured.

Out of scope: `fetch` materialization to markdown (next story); `--since`; remote adapters; TUI/web.

## Acceptance Criteria

- **Given** `comment_store = "git-ref"`, **when** I run `comment add <doc> --body "x"`, **then** a blob is written and a ref under `refs/lazyspec/comments/<doc>/` points at it — with no commit and no branch pollution.
- **Given** two concurrent writers to the same thread, **then** the `update-ref` CAS yields exactly one winner per ref and neither corrupts the other — distinct ULIDs never contend.
- **Given** `comment_store = "git-ref"`, **when** I run `comments <doc> --json`, **then** the folded thread is rendered via `git cat-file` (comments are not files on disk in this store).
- **Given** a per-type `comment_store` override, **then** that type's documents use the overriding store while others use the project default.
- The engine fold is store-agnostic — the same fold serves filesystem and git-ref.

## Notes

Trade-off: git-ref comments are opaque (not `cat`-able) and GitHub rejects pushes to `refs/lazyspec/*` — so this store is local / self-hosted / bare-remote. Cross-host sharing is the `fetch`-materialize story.

