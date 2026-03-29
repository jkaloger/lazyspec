---
title: GitHub Issues TUI integration
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-037
---




## Context

GitHub Issues documents are integrated into the TUI alongside filesystem and git-ref documents. Users can edit issues directly in their configured editor, with changes synchronized back to GitHub on save. The unified document engine handles all three sources seamlessly, while background cache refreshes keep data current and sync status indicators provide visibility into document freshness.

## Acceptance Criteria

### AC: GitHub Issues appear in document list

Given the TUI is displaying documents from multiple sources
When a user views the document list
Then GitHub Issues documents appear alongside filesystem and git-ref documents

### AC: Open and edit issue in external editor

Given a GitHub Issues document is selected
When the user presses `e`
Then the document opens in $EDITOR and on close, changes are pushed to GitHub with an optimistic lock check

### AC: Optimistic lock conflict detection

Given a document has been fetched and modified locally
When the user saves and the TUI attempts to push
Then if a conflict is detected, the user is warned before overwriting

### AC: Cycle issue status

Given a GitHub Issues document is selected
When the user presses `s`
Then the status cycles between open and closed states for lifecycle issues, or updates frontmatter for non-lifecycle documents

### AC: Sync indicator in status bar

Given documents are being synchronized with GitHub
When the TUI is running
Then the status bar displays a sync indicator showing the last fetch timestamp

### AC: Warning for stale cached documents

Given cached documents exist
When documents have not been refreshed for more than 2x their TTL
Then a warning indicator appears on affected documents

### AC: Background cache refresh

Given the TUI is running
When the poll cycle executes
Then stale documents are automatically refreshed from the cache backend

## Scope

### In Scope

- GitHub Issues document display in TUI alongside other document sources
- Editor integration with optimistic locking and conflict detection
- Status cycling for issue lifecycle management
- Sync status indicator in status bar
- Warning indicators for stale cached documents
- Background cache refresh during TUI poll cycles

### Out of Scope

- Native HTTP client implementation for background refresh
- Rate limit UI or throttling visualization
