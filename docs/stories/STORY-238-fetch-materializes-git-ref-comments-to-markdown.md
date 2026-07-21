---
title: Fetch materializes git-ref comments to markdown
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer, I want `lazyspec fetch` to materialize git-ref comments into committed markdown, so that ephemeral live chatter becomes a durable, `cat`-able, PR-reviewable artifact I can share.

Extends the existing `fetch` command (which already pulls git-ref / remote documents into local markdown) one layer down to comments — no bespoke `comment materialize` verb (convention principle 5: reuse the norm).

## Scope

- `lazyspec fetch` folds ref-backed comment streams into committed maildir markdown files under each document's `comments/` directory, alongside the documents it already fetches.
- `--type <doc-type>` filter is honoured for comments as it is for documents.
- Materialized comments are ordinary markdown (the filesystem-store shape), so all existing readers work.

Out of scope: `--since` cursor (next story); native GitHub/ClickUp comment fetch (their own stories).

## Acceptance Criteria

- **Given** a document with git-ref comments, **when** I run `lazyspec fetch`, **then** each ref-backed comment is written as a ULID-named markdown file under the doc's `comments/` directory, `cat`-able and PR-reviewable.
- **Given** a materialized thread, **when** I run `comments <doc>`, **then** the folded result matches the ref-backed fold (materialization is loss-free for envelope + attributes).
- **Given** `fetch --type rfc`, **then** only rfc documents' comments are materialized.
- **Given** a comment already materialized, **when** I run `fetch` again, **then** it is not duplicated (idempotent by ULID filename).

## Notes

This is the bridge from the live lane (git-ref) to the durable/shared lane (committed markdown). Which store is live vs durable is configuration, not hard-coded policy. Cross-GitHub sharing goes through this materialized markdown, not raw refs.

