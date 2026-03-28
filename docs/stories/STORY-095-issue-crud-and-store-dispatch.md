---
title: Issue CRUD and store dispatch
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-037-github-issues-document-store.md
---


## Context

When a document type has `store = "github-issues"`, lazyspec CLI commands route through the gh CLI integration layer instead of writing to the filesystem. This story wires up the store dispatch mechanism, issue number mapping, optimistic locking checks, and status-based issue lifecycle management as defined in RFC-037.

## Acceptance Criteria

### AC: Create via github-issues store

Given a document type with `store = "github-issues"`
When `lazyspec create` is executed with that type
Then lazyspec creates a GitHub issue via the gh CLI, records the issue number and updated_at timestamp in `.lazyspec/issue-map.json`, and writes the cache file

### AC: Update via github-issues store with optimistic lock

Given a document stored in github-issues with a known updated_at timestamp
When `lazyspec update` is executed
Then lazyspec compares the document's updated_at against the remote version, fails if remote has been modified since last fetch, and applies the update via gh CLI only if timestamps match

### AC: Delete and lifecycle management

Given a document stored in github-issues
When `lazyspec delete` is executed
Then lazyspec closes the issue, removes the `lazyspec:{type}` label, and prepends [DELETED] to the issue title

### AC: Status mapping on writes

Given a document with a status field
When status is set to `complete` and the document is synced
Then the issue is closed on GitHub

When status is set to `draft` and the document is synced
Then the issue is reopened on GitHub

### AC: Store dispatch routing

Given multiple document types with different `store` values
When a create/update/delete command is executed
Then the engine routes to the correct backend (filesystem or github-issues) based on the type's `store` field

### AC: Issue number mapping

Given documents in github-issues storage
Then `.lazyspec/issue-map.json` maintains a mapping of document ID to issue number and updated_at timestamp for all stored documents

## Scope

### In Scope

- Store dispatch: routing create/update/delete operations based on document type's `store` field
- Issue number mapping: tracking document ID to GitHub issue number and updated_at in `.lazyspec/issue-map.json`
- Optimistic locking: comparing updated_at timestamps on update to prevent lost writes
- Status mapping: closing/reopening issues based on status field changes
- Delete operation: closing issue, removing label, marking as deleted in title
- Integration with gh CLI for all mutations

### Out of Scope

- Caching and TTL logic
- TUI improvements
- Init and setup workflows
- Cross-backend relationships
- Performance optimization
