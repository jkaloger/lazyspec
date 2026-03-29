---
title: Cross-backend relationship resolution
type: story
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-037
---



## Context

Documents can link to each other across storage backends. A github-issues iteration can implement a filesystem story. The engine resolves relationships by looking up document IDs in a unified index that spans filesystem, git-ref, and github-issues backends. `lazyspec context` renders the full relationship chain and `lazyspec validate` checks relationship integrity across backends.

## Acceptance Criteria

### AC: Unified document index loads all backends

Given documents exist in filesystem, git-ref, and github-issues backends
When the unified index is built
Then all documents from all configured backends are indexed and available for lookup

### AC: Cross-backend relationship resolution

Given a document in one backend references a document in another backend
When the relationship target is resolved
Then the target document is found and returned, regardless of backend type

### AC: Context command follows cross-backend chains

Given a document with relationships across backends
When `lazyspec context <id>` is executed
Then the full relationship chain is displayed with backend type indicated for each document

### AC: Validate detects broken cross-backend relationships

Given a document references a target that does not exist
When `lazyspec validate` is executed
Then a validation error is reported showing the broken reference and source document

### AC: Show command works with expanded relationships for all backend types

Given a github-issues document with relationships
When `lazyspec show <id>` is executed with relationship expansion enabled
Then relationships are resolved and displayed, including linked documents from other backends

## Scope

### In Scope

- Unified document index that aggregates filesystem, git-ref, and github-issues backends
- Cross-backend relationship resolution and target lookup
- Backend type information in context output
- Validation of relationship targets across backends
- Relationship expansion in show command for documents from any backend

### Out of Scope

- Relationship editing UI or CLI for cross-backend relationships
- New relationship types beyond existing `implements`, `references`, etc.
- Caching strategies or performance optimization of index lookups
