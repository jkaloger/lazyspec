---
title: GitHub issue comments read-through
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

A lazyspec document backed by a GitHub issue today shows only the lazyspec-authored
body: lazyspec writes the issue body (with its metadata HTML comment), and `show --json`
/ `status --json` surface the doc fields from that authored body. The human conversation
that accumulates on a GitHub issue — the comment thread — is invisible to lazyspec.

This slice (story 6 of RFC-050) makes that thread readable. It fetches GitHub issue
comments and surfaces them as a read-only `comments` array in `show --json` and
`status --json`. The binding is deliberately one-directional: comments are read, never
authored. They are never merged into the lazyspec-authored `--body` and never round-tripped
back to GitHub, which keeps the body serialization clean and the metadata HTML comment the
single source of authored content.

It ships independently of the Projects work — no project board, milestone, or attribute
machinery is required, and it needs no GraphQL: comments are REST, fetched via
`GET /repos/{o}/{r}/issues/{n}/comments` through the existing `GhIssueReader` trait seam
(per RFC-050, comments stay on the existing reader). It builds only on the shared
github-access plumbing landed in STORY-155, so the engine stays testable behind the trait
fake. This is the smallest possible increment that gives a github-backed doc visibility into
its own discussion thread.

## Acceptance Criteria

- **Given** a github-backed document whose issue has two comments
  **When** `show --json` is run for that document
  **Then** the output includes a `comments` array of length two, each entry carrying
  `author`, `body`, and `timestamp` fields populated from GitHub.

- **Given** a github-backed document whose issue has comments
  **When** `status --json` is run for that document
  **Then** the output includes the same read-only `comments` array (author/body/timestamp).

- **Given** a github-backed document whose issue has comments
  **When** the comments are fetched and the doc is serialized
  **Then** the lazyspec-authored `body` field is byte-for-byte unchanged — no comment text
  is merged into the body and no comment is written back to GitHub.

- **Given** a filesystem-backed document (no GitHub issue)
  **When** `show --json` or `status --json` is run
  **Then** the output either omits the `comments` key or emits an empty `comments` array,
  and no GitHub fetch is attempted.

- **Given** a github-backed document whose issue has zero comments
  **When** `show --json` is run
  **Then** the `comments` array is present and empty.

- **Given** the comments feature
  **When** the codebase is inspected for any comment-posting/editing/deleting path
  **Then** none exists — there is no CLI flag, store method, or mutation that writes a
  comment; the binding is read-only.

## Scope

### In Scope

- Fetch GitHub issue comments (author, body, timestamp) for a github-backed document via
  REST `GET .../issues/{n}/comments` on the existing `GhIssueReader` trait seam (no GraphQL).
- A read-only `comments` array surfaced in `show --json` and `status --json`, wired through
  the doc-to-JSON serialization (`src/cli/json.rs`).
- Comments kept entirely separate from the authored `--body`: never merged, never
  round-tripped to GitHub.
- Filesystem-backed docs produce no `comments` (absent or empty) and trigger no fetch.
- Engine-level coverage with the `Gh*`/`GhGraphql` trait fake driving the fetch, plus a
  filesystem case asserting no comments and no fetch.

### Out of Scope

- Posting, editing, or deleting comments — any write path. (RFC-050 keeps comments
  read-only; deferred to a later RFC.)
- Rendering comments in the TUI; JSON-only is sufficient here. (Possible follow-up.)
- Comment reactions, edit history, or threaded/review-comment metadata.
- Merging or round-tripping comments through the issue-body HTML comment.
- Caching comments in the schema snapshot or `.lazyspec/cache`; this slice fetches on read.
