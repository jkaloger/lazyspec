---
title: Comment read path and ordering fold
type: iteration
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: STORY-232
---

## Objective

List a document's comments, ULID-ordered, in both `--json` and a pretty terminal view.

## Satisfies

STORY-232 AC3 (`--json`, distinct entries, ULID order) and AC4 (pretty print: author + relative time). Depends on ITER-A (write path + envelope).

## Context

- Story + ACs: STORY-232.
- Output-parity rule + envelope: RFC-060 §Interfaces ("Output modes"), §Design (envelope).
- Conventions: `docs/convention/DICTUM-006-cli-patterns.md` (`--json` + pretty from same data), `docs/convention/DICTUM-004-testing.md` (TDD).
- Touch:
  - `src/engine/comment.rs` — read all comments for a doc + ordering fold (sort by ULID). No threading/resolution fold here.
  - `src/cli/comment.rs` — `comments` handler: `--json` (envelope + attribute map per entry) and default pretty (author + relative time per entry).
  - `src/cli.rs` — add `comments <doc>` to the `Commands` enum.

## Tasks

1. Test-first: engine test — two appended comments read back as distinct entries in ULID order; CLI test — `--json` shape (envelope + attrs) and pretty output both derive from the same folded list.
2. Implement the read + ordering fold (sort-by-ULID) in `src/engine/comment.rs`.
3. Wire `comments <doc>` CLI with `--json` and pretty (relative-time formatting consistent with existing CLI output; see `src/cli/status.rs` / `src/cli/show.rs`).

## Out of scope

- Threading tree (`in_reply_to` fold) → STORY-233; resolution fold → STORY-236.
- `--since` cursor → STORY-239.
- TUI pane / web tree → STORY-242 / STORY-243.

## Principles/conventions

Pointers only: DICTUM-006 (both views render the same folded data — convention principle 2), DICTUM-004 (TDD).

## Verification

After two `comment add`s to a doc, `lazyspec comments <doc> --json` returns both entries ULID-ordered with envelope + attribute map; `lazyspec comments <doc>` prints each with author + relative timestamp.

