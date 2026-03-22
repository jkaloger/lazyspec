---
title: Git status gutter in main document list
type: story
status: accepted
author: agent
date: 2026-03-23
tags: []
related:
- implements: docs/rfcs/RFC-031-git-status-gutter-in-tui.md
---



## Context

The TUI document list currently gives no indication of which files have uncommitted git changes. Users have to switch to a terminal or run `git status` separately to know what they've modified. This story adds a git-signs-style colored gutter to the left of the primary document tree view, providing at-a-glance change status.

## Acceptance Criteria

- Given a document that is untracked or newly added to git,
  when the main document list is rendered,
  then a green bar appears in the gutter column for that document's row.

- Given a document whose content has been modified since the last commit,
  when the main document list is rendered,
  then a yellow bar appears in the gutter column for that document's row.

- Given a document with no uncommitted changes,
  when the main document list is rendered,
  then the gutter column for that row is empty.

- Given a partially staged document (some changes staged, some unstaged),
  when the main document list is rendered,
  then a yellow bar appears in the gutter column (treated as modified).

- Given the TUI is opened inside a git repository,
  when the main document list first renders,
  then `git status --porcelain` is called once and the result is cached.

- Given a cached git status exists,
  when a document is created, saved, or deleted,
  then the cache is invalidated and refreshed on the next render.

- Given the TUI is opened outside a git repository,
  when the main document list is rendered,
  then no gutter indicators are shown and no errors are raised.

- Given a document that was renamed according to `git status --porcelain`,
  when the main document list is rendered,
  then a yellow bar appears in the gutter column for that document's row.

## Scope

### In Scope

- Querying git status via `git status --porcelain`
- Parsing porcelain output to classify files as new/untracked, modified, or unchanged
- Synchronous caching of git status, invalidated on document create/save/delete events
- Rendering a single-character gutter column as the leftmost column in the main document tree view
- Green gutter for new/untracked files, yellow for modified files
- Handling non-git-repo and renamed-file edge cases

### Out of Scope

- Gutter rendering in search results or filtered views (Story 2)
- Status field diffing against HEAD (comparing YAML frontmatter)
- Background/async refresh of git status cache
- Deleted file indicators (deleted files do not appear in the document list)
