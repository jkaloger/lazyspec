---
title: clickup fetch requests markdown description not plaintext
type: iteration
status: complete
author: Jack Kaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-198
---

## Objective

ClickUp task fetch returns full markdown body (lists/headings/badges preserved), not plaintext-flattened `text_content`.

## Context

- Story: STORY-198 (read path). RFC-056 §Field mapping.
- Root cause: fetch URLs omit ClickUp's `include_markdown_description=true` query param. Without it the API leaves `markdown_description` empty → body decode (`clickup_cache.rs:200-204`) falls back to plaintext `text_content` → all markdown formatting stripped.
- Touch: `src/engine/clickup.rs` — `task_list` URL `:472-475`; `get_task` URL `:560`.
- Decode fallback stays as-is (`clickup_cache.rs:200-204`): correct — keeps `text_content` fallback for tasks with genuinely empty markdown.

## Satisfies

STORY-198 read path — defect fix, no new AC. Bodies materialize as ClickUp markdown source, not flattened text.

## Tasks

1. Append `&include_markdown_description=true` to `task_list` URL (`clickup.rs:473`, has query string).
2. Append `?include_markdown_description=true` to `get_task` URL (`clickup.rs:560`, no query string yet).

## Out of scope

- Body decode logic (`clickup_cache.rs:200-204`) — already correct.
- Write path `markdown_content` — unaffected.
- HTTP wire-level test infra — none exists (only `FakeClickupClient` trait fake, `clickup.rs:619`); URL strings are untested by design. One-line const-URL change follows sibling `task_list`/`list_statuses` pattern; do NOT add a mock-server dep for it (dictum 6). Verify via real token.

## Principles

- Traits at seam: `ClickupClient` trait, fake in tests (dictum 4).
- No indirection for one use (dictum 6).

## Verification

Fetch a ClickUp task whose description has a heading + list + badge → local cache doc body shows markdown source, not flattened plaintext.

