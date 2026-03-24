---
title: "Git Status Integration"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, git]
related:
  - related-to: docs/stories/STORY-077-git-status-gutter-in-main-document-list.md
  - related-to: docs/stories/STORY-078-git-status-gutter-in-search-and-filtered-views.md
---

## Acceptance Criteria

### AC: new-file-green-gutter-in-tree

Given a document file is untracked or newly added in git
When the main document tree is rendered
Then a green `┃` character appears in the gutter column for that row

### AC: modified-file-yellow-gutter-in-tree

Given a document file has been modified since the last commit
When the main document tree is rendered
Then a yellow `┃` character appears in the gutter column for that row

### AC: unchanged-file-blank-gutter

Given a document file has no uncommitted git changes
When any document list view is rendered
Then the gutter column for that row contains a blank space

### AC: renamed-file-treated-as-modified

Given a document file has been renamed according to `git status --porcelain` (status code `R ` or `RM`)
When the gutter is rendered
Then the destination path receives a yellow `┃` indicator (classified as `Modified`)

### AC: porcelain-parsing-rejects-short-lines

Given a line from `git status --porcelain` output is shorter than 4 bytes
When `parse_porcelain_line` processes it
Then it returns `None` and the line is silently skipped

### AC: cache-populated-on-startup

Given the TUI is opened inside a git repository
When `App::new` constructs the `GitStatusCache`
Then `git status --porcelain` is called once and the results are stored in the cache's `HashMap`

### AC: cache-invalidated-on-file-change

Given a cached git status exists
When a `FileChange` event is processed in the event loop
Then `git_status_cache.invalidate()` is called, setting the stale flag to `true`

### AC: cache-invalidated-on-create-complete

Given a cached git status exists
When a `CreateComplete` event is successfully processed
Then `git_status_cache.invalidate()` is called, setting the stale flag to `true`

### AC: cache-refresh-before-render

Given the git status cache has been invalidated (stale flag is `true`)
When the `draw` function executes at the start of a render cycle
Then `git_status_cache.refresh()` re-runs `git status --porcelain` and clears the stale flag before any view reads the cache

### AC: refresh-noop-when-not-stale

Given the git status cache has not been invalidated
When `refresh()` is called
Then no subprocess is spawned and the cached data is unchanged

### AC: search-overlay-gutter-new-file

Given a search result list is displayed and a result file is untracked or newly added in git
When the search overlay renders that row
Then a green `┃` span appears as the leading character of that list item

### AC: search-overlay-gutter-modified-file

Given a search result list is displayed and a result file has been modified in git
When the search overlay renders that row
Then a yellow `┃` span appears as the leading character of that list item

### AC: filtered-view-gutter-consistency

Given a filtered document list is displayed (e.g., filtered by tag or type)
When the cached git state contains changed files in the filtered set
Then the gutter indicators match those shown in the main document tree for the same files

### AC: non-git-repo-no-gutter

Given the TUI is opened in a directory that is not inside a git repository
When any document list view is rendered
Then all gutter positions render as blank spaces and no errors are raised
