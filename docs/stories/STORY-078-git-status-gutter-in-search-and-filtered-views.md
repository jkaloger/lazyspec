---
title: Git status gutter in search and filtered views
type: story
status: accepted
author: agent
date: 2026-03-23
tags: []
related:
- implements: RFC-031
---




## Context

Story 1 adds git status querying, caching, and gutter rendering to the primary document tree view. This story extends the same gutter to search results and filtered document lists so that git change indicators are visible regardless of how the user is browsing documents.

The gutter reuses the cached git state from Story 1. No additional git commands are executed.

## Acceptance Criteria

- Given a search result list is displayed,
  when the cached git state contains a new/untracked file that appears in the results,
  then a green gutter bar renders to the left of that document row.

- Given a search result list is displayed,
  when the cached git state contains a modified file that appears in the results,
  then a yellow gutter bar renders to the left of that document row.

- Given a search result list is displayed,
  when a file in the results has no git changes,
  then no gutter indicator renders for that row.

- Given a filtered document list is displayed (e.g. filtered by tag or type),
  when the cached git state contains changed files in the filtered set,
  then the appropriate green or yellow gutter bars render for those rows.

- Given the git state cache has not been invalidated,
  when the user switches between the main list, search results, and filtered views,
  then the gutter indicators remain consistent across all views without re-querying git.

- Given a document appears in search results but exists outside a git repository,
  when the list renders,
  then no gutter indicator is shown for that document.

## Scope

### In Scope

- Rendering gutter indicators in search result views
- Rendering gutter indicators in filtered document list views
- Reading from the shared git state cache (established in Story 1)
- Consistent gutter appearance (single character, green/yellow) matching the main list

### Out of Scope

- Git status querying and caching logic (Story 1)
- Gutter rendering in the primary document tree (Story 1)
- Status-field diffing
- Handling deleted files in the gutter
- Any new git commands or cache invalidation logic
