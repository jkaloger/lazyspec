---
title: Post a comment on a document and read it back
type: story
status: accepted
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---## Story

As a document author or reviewer, I want to post an attributed comment on a document and read it back, so that I can annotate the document without rewriting its body.

This is the walking skeleton: the thinnest end-to-end path through the comment layer — write one comment, list it — on the filesystem maildir store, with the engine `Comment` envelope and the ordering fold in place. Threading, configurable attributes, resolution, and other stores are later slices layered on top.

## Scope

- `Comment` envelope: `id` (ULID), `author`, `created_at`, `in_reply_to` (null here), `refs`, plus an opaque `attributes` map persisted verbatim.
- Filesystem maildir store: one comment per ULID-named file under `<doc-dir>/comments/`.
- `lazyspec comment add <doc> --body <..> | --body-file -` — append a comment (inline body or stdin).
- `lazyspec comments <doc>` — list the doc's comments, `--json` (envelope + attribute map) and default pretty print (author + relative time per entry).
- Comments are immutable once written.

Out of scope (later stories): reply threading, attribute schema/validation, resolution, git-ref/remote stores, `--since`, TUI/web.

## Acceptance Criteria

- **Given** a document `RFC-060`, **when** I run `lazyspec comment add RFC-060 --body "note"`, **then** a new ULID-named markdown file is created under the document's `comments/` directory carrying the envelope (id, author, created_at, in_reply_to=null) and body.
- **Given** a body piped on stdin, **when** I run `lazyspec comment add RFC-060 --body-file -`, **then** the comment is created with the piped body (parity with `--body`).
- **Given** two comments posted to the same document, **when** I run `lazyspec comments RFC-060 --json`, **then** both are returned as distinct entries ordered by ULID, each with its envelope and attribute map.
- **Given** the same document, **when** I run `lazyspec comments RFC-060` without `--json`, **then** each comment prints with author and relative timestamp.
- **Given** a non-existent document id, **when** I run `lazyspec comment add NOPE-999 --body "x"`, **then** the command fails with a clear error (`--json` diagnostic) and writes nothing.
- **Given** neither `--body` nor `--body-file` (or an empty body), **when** I run `lazyspec comment add RFC-060`, **then** the command fails with a missing-body error and writes nothing.

## Notes

- Envelope is engine-owned; `attributes` is an opaque map the store persists verbatim. Only the filesystem store exists at this slice — no `CommentStore` trait yet (convention principle 6: introduced when the second store lands, STORY-237 for git-ref).
- The fold at this slice is **ordering only** (sort by ULID). Threading (`in_reply_to`) and resolution folds are deferred to STORY-233 and STORY-236 respectively.
- `refs` is carried on the envelope for completeness but is **deliberately unpopulated** here — no CLI sets it and no AC covers it at the skeleton.
- **NFR:** the `--json` and pretty views render from the same folded data (convention principle 2).
