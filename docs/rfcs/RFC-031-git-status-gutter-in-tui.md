---
title: "Git Status Gutter in TUI"
type: rfc
status: accepted
author: "agent"
date: 2026-03-23
tags: []
---


## Summary

Add a minimal colored bar gutter to the left of document lists in the TUI, showing git change status (new, modified) across all views and modes.

## Intent

Users need immediate visual feedback about which documents have uncommitted changes in their working tree. Currently, there's no indication in the TUI of whether a document is new or modified since the last commit. This makes it harder to track what's changed during an editing session.

The git status gutter mirrors the git-signs pattern from vim/nvim plugins: a thin, colored vertical bar on the left side of each document row that indicates its git state without consuming significant screen real estate.

## Design

### Gutter Appearance

A single-character-width gutter to the left of the document tree, containing a colored bar based on file state:

```
┃ • ID    Title               Status   Tags
┃ •
┃
┃ •
```

### Color Coding

- Green (`Color::Green`): File is new or untracked (not yet committed)
- Yellow (`Color::Yellow`): File content is modified since last commit
- No color/empty: No git changes detected

Partially staged files are treated as "modified." Deleted files are excluded from the gutter (they don't appear in the document list).

### Gutter Position

The gutter appears as the leftmost column in all document list views, before the existing tree indicators (├─, └─, ▶, ▼).

### Affected Views

The gutter should be rendered consistently across:

1. Main document list (primary view)
2. Search results
3. Filtered document views
4. Any mode that displays documents in a tabular/tree format
5. Virtual document lists (if applicable)

### Implementation Considerations

#### Git State Detection

Run `git status --porcelain` once to get all file states. Parse the output to classify each document file as new/untracked, modified, or unchanged. This single call covers staged, unstaged, and untracked files.

#### Caching

Git queries are I/O bound. The cache holds the result of the last `git status` call and is invalidated when a TUI event might change files (document create, save, delete). Between invalidation events, the cached state is reused across renders.

Start with synchronous caching: query git on first render after invalidation, reuse until the next invalidating event. A background refresh thread can be added later if synchronous queries cause perceptible lag on large repos, but this is unlikely for the repository sizes lazyspec targets.

#### Edge Cases

- Documents outside a git repository: no gutter indicator
- Renamed files: `git status --porcelain` reports these; treat as modified

## Stories

This RFC identifies the following vertical slices:

1. Gutter in main document list: query git status, cache results, render green/yellow gutter indicators in the primary document tree view. Delivers the full feature for the most common view.
2. Gutter in search and filtered views: extend the gutter to search results and filtered document lists, reusing the cached git state from Story 1.

> [!NOTE]
> Status field change detection (comparing YAML frontmatter between working tree and HEAD) is out of scope for this RFC. It requires parsing frontmatter from git objects and solves a different problem. It can be proposed as a follow-up RFC if there's demand.
