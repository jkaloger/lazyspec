---
title: Hybrid cache and fetch
type: story
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-037-github-issues-document-store.md
---


## Context

The github-issues document store requires an efficient read path that balances API availability with responsiveness. Cached documents stored in `.lazyspec/cache/{type}/{id}.md` should serve reads when fresh. Timestamps in `cache.lock` track freshness. When cache becomes stale within a configured TTL, fresh data is fetched from the GitHub API using `gh` CLI. The `lazyspec fetch` command forces a full refresh. When the API is unreachable, stale cache degrades gracefully with a warning.

## Acceptance Criteria

### AC: Fresh cache hit returns cached file without API call

**Given** a cached github-issues document within its TTL
**When** a read request is made
**Then** the cached file is returned without making an API call

### AC: Stale cache triggers fetch and refresh

**Given** a cached document past its TTL
**When** a read request is made
**Then** the document is fetched from GitHub API, cache file and cache.lock are updated with fresh content and timestamp, and fresh content is returned

### AC: Cold cache fetches from API

**Given** no cached document exists
**When** a read request is made
**Then** the document is fetched from GitHub API, cache file is written to `.lazyspec/cache/{type}/{id}.md`, and content is returned

### AC: Fetch command refreshes all documents

**Given** a repository with cached github-issues documents
**When** `lazyspec fetch` is executed
**Then** all github-issues documents are refreshed from the API regardless of TTL

### AC: Fetch command uses label filtering and pagination

**Given** the fetch operation is running
**When** querying the GitHub API
**Then** `gh issue list` is called with `lazyspec:{type}` label filter and pagination is handled for large result sets

### AC: Offline degradation with stale cache

**Given** cached documents exist but the API is unreachable
**When** a read request is made
**Then** the stale cache is returned with a warning message indicating the content may be outdated

### AC: Cache structure with timestamps

**Given** caching is implemented
**When** documents are cached
**Then** files are stored at `.lazyspec/cache/{type}/{id}.md` and `cache.lock` contains timestamps for each document

### AC: Removed issues are cleaned up

**Given** `lazyspec fetch` is executed
**When** issues previously cached no longer exist in the API
**Then** those cached files are removed and entries are deleted from the issue map

## Scope

### In Scope

- TTL-based freshness checking with timestamp tracking in cache.lock
- Cache read path: check freshness, fetch if stale, return content
- Cold cache handling: fetch and initial write
- Offline degradation: return stale cache with warning when API unreachable
- Cache directory structure: `.lazyspec/cache/{type}/{id}.md`
- `lazyspec fetch` full refresh with label filtering and pagination
- Cleanup of removed issues during fetch

### Out of Scope

- TUI background refresh
- Rate limit management and backoff strategies
- Native HTTP client (using `gh` CLI only)
- Cache invalidation policies beyond TTL
- Compression or deduplication of cached content
