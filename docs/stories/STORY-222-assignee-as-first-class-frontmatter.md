---
title: Assignee as first-class frontmatter
type: story
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- related-to: RFC-049
---

## Value

As a lazyspec user on a team, I see who owns any document — assignee is a first-class frontmatter field on every doc, inherited from the remote on GitHub-issues/ClickUp-backed types and set directly on filesystem/git-ref types — so ownership is visible without opening the remote tool.

## Acceptance Criteria

- AC1: `assignee` is a first-class frontmatter field (like status/tags), parsed and serialized on all stores; surfaced in `show --json` and `status --json`.
- AC2: filesystem/git-ref: settable via CLI (`update <id> --assignee <name>`), never by hand-editing.
- AC3: github-issues-backed docs inherit the issue's assignee on sync; clickup-backed docs inherit the task's assignee. Remote is source of truth for those stores.
- AC4: setting assignee via lazyspec on a remote-backed doc writes through to the remote (same write-through model as body/status).
- AC5: TUI, web view, and CLI all display assignee (list column and detail view) — feature parity across the three surfaces.
- AC6: docs without an assignee stay valid — field optional, absent from frontmatter when unset.

## Out of scope

- Assignee-based filtering/queries (later slice).
- Multi-assignee (single assignee first; ClickUp multi-assign maps to first or is deferred).
- User identity mapping between github/clickup/git identities.
