---
title: Resolve a comment thread
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer, I want to resolve a comment thread, so that a discussion is marked settled without erasing the reasoning that led there.

Resolution is itself an append — a comment carrying the configured resolution attribute — and resolved-state is folded from a config-driven predicate, never written as a mutable field.

## Scope

- `lazyspec comment resolve <thread-root>` — appends a comment referencing the root, carrying the configured resolution attribute (default `kind=decision`).
- `resolution` config: the predicate that marks a thread resolved (default `kind == decision`).
- The fold computes resolved-state per thread from the predicate.
- `comments <doc>` surfaces resolved state in `--json` and pretty output.

Out of scope: collapse-resolved rendering in TUI/web (those stories); GC of resolved threads (deferred past v1).

## Acceptance Criteria

- **Given** a thread root `C1`, **when** I run `lazyspec comment resolve C1`, **then** a new comment is appended `in_reply_to`/referencing `C1` carrying the resolution attribute — and `C1` itself is unchanged.
- **Given** a config `resolution` predicate `kind == decision`, **when** a thread contains a comment with `kind=decision`, **then** the folded thread reports resolved.
- **Given** a config that sets the resolution predicate to a different attribute, **when** that attribute appears, **then** resolved-state folds from the new predicate — no compiled-in policy.
- **Given** a resolved thread, **when** a further reply is appended, **then** the thread and its history remain intact (append-only; resolution is not a terminal lock).
- Resolved state appears in both `--json` and pretty output.

## Notes

Event sourcing: resolution status is derived, not stored. What counts as resolved is project policy.

