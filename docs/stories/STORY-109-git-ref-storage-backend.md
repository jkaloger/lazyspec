---
title: Git-ref storage backend
type: story
status: accepted
author: jkaloger
date: 2026-04-01
tags: []
related:
- implements: RFC-035
---



## Context

RFC-035 introduces git-ref-based document storage so that lazyspec documents can be shared across agents and machines without polluting the working tree. This story covers the storage backend itself: the `GitRefStore` that persists documents as commit chains under `refs/lazyspec/{type}/{id}`, the local shadow cache that materializes those documents for reads, the `lazyspec fetch` command, and the integration points that let git-ref documents participate in listing, search, validation, and cross-backend relationships.

This story depends on Story 1 (GitRefOps trait and lease engine), which provides the `GitRefOps` trait, `GitCli` implementation, and `MockGitRefClient` for testing.

## Acceptance Criteria

### StoreBackend::GitRef variant and config parsing

- **Given** a lazyspec project with a document type configured with `store = "git-ref"`
  **When** the config is loaded
  **Then** the type definition has `StoreBackend::GitRef` and is accepted without error

- **Given** a lazyspec project with no git-ref types configured
  **When** the config is loaded
  **Then** no `StoreBackend::GitRef` types exist and other backends are unaffected

### GitRefStore DocumentStore implementation

- **Given** a `GitRefStore<R>` with a valid `GitRefOps` implementation
  **When** `create` is called with a document type and content
  **Then** a new ref is created at `refs/lazyspec/{type}/{id}` containing a commit whose tree holds the document markdown, and the shadow cache is updated

- **Given** an existing git-ref document
  **When** `update` is called with new content
  **Then** a new commit is created parented on the previous commit for that ref, the ref is updated via CAS, and the shadow cache is refreshed

- **Given** an existing git-ref document
  **When** `update` is called but the ref has been updated concurrently (CAS mismatch)
  **Then** the operation fails with a conflict error

- **Given** an existing git-ref document
  **When** `delete` is called
  **Then** the ref is deleted, the cache file is removed, and the `cache.lock` entry is removed

### Shadow cache structure and cache.lock

- **Given** a git-ref document that has been created or fetched
  **When** the shadow cache is inspected
  **Then** the document exists at `.lazyspec/cache/{type}/{id}.md` and `cache.lock` contains a JSON entry mapping that document to its ref SHA

- **Given** a `cache.lock` file
  **When** it is read
  **Then** it is valid JSON mapping document paths to ref SHAs

- **Given** the `.lazyspec/cache/` directory
  **When** the project `.gitignore` is checked
  **Then** the cache directory is gitignored

### lazyspec fetch command

- **Given** a remote with git-ref documents
  **When** `lazyspec fetch` is run
  **Then** local refs are updated from the remote, cache files are written for refs whose SHA differs from `cache.lock`, and `cache.lock` is updated

- **Given** a remote where a git-ref document has been deleted
  **When** `lazyspec fetch` is run
  **Then** the local ref is removed, the cache file is deleted, and the `cache.lock` entry is removed

- **Given** no remote git-ref documents exist
  **When** `lazyspec fetch` is run
  **Then** the command completes without error and the cache is empty

### Store::load_with_fs dispatch for GitRef

- **Given** a document type with `store = "git-ref"`
  **When** `Store::load_with_fs` is called
  **Then** it reads from `.lazyspec/cache/{type}/` the same way it reads filesystem documents

### dispatch_for_type extension

- **Given** the unified document engine
  **When** a mutation targets a git-ref document type
  **Then** `dispatch_for_type` routes to `GitRefStore` via the third `Option<&mut GitRefStore<R>>` parameter

- **Given** a mutation targeting a filesystem or github-issues document type
  **When** `dispatch_for_type` is called with a `GitRefStore` parameter present
  **Then** the mutation routes to the correct backend and `GitRefStore` is not invoked

### Cross-backend reads (list, show, search, validate, context, status)

- **Given** documents stored across filesystem, github-issues, and git-ref backends
  **When** `lazyspec list` is run
  **Then** documents from all three backends appear in the output

- **Given** a git-ref document
  **When** `lazyspec show {id}` is run
  **Then** the document content is displayed from the shadow cache

- **Given** documents across all backends
  **When** `lazyspec search {query}` is run
  **Then** results include matches from git-ref documents

- **Given** documents across all backends
  **When** `lazyspec validate` is run
  **Then** git-ref documents are validated alongside filesystem and github-issues documents

- **Given** a document chain spanning backends (e.g., filesystem RFC, git-ref story)
  **When** `lazyspec context {id}` is run
  **Then** the full chain is resolved across backends

- **Given** documents across all backends
  **When** `lazyspec status` is run
  **Then** git-ref documents are included in the status output

### Cross-backend relationship resolution

- **Given** a git-ref document linked to a filesystem document
  **When** the relationship is resolved
  **Then** both sides of the link are accessible regardless of backend

- **Given** a filesystem document linked to a git-ref document
  **When** `lazyspec context` or `lazyspec show` resolves relationships
  **Then** the git-ref document is loaded from the shadow cache (or via direct ref read on cold cache)

### Cold cache fallback

- **Given** a git-ref document that exists as a ref but has no shadow cache entry
  **When** a read operation (show, list, context) targets that document
  **Then** the system falls back to reading the blob directly via `GitRefOps::read_ref_blob` and materializes the cache entry

- **Given** a completely empty cache (no `cache.lock`, no cache files)
  **When** a read operation encounters a git-ref type
  **Then** it reads refs directly, materializes the cache, and serves the documents

## Scope

### In Scope

- `StoreBackend::GitRef` variant on the enum
- `GitRefStore<R: GitRefOps>` implementing `DocumentStore` (create/update/delete via commit-chain CRUD on `refs/lazyspec/{type}/{id}`)
- Shadow cache in `.lazyspec/cache/{type}/` with `cache.lock` JSON
- `Store::load_with_fs` dispatch arm for `GitRef` (reads from `.lazyspec/cache/{type}/`)
- `dispatch_for_type` extended with third `Option<&mut GitRefStore<R>>` parameter
- `lazyspec fetch` command to materialize cache from refs
- Extend `list/show/search/validate/context/status` to operate across all three backends
- Cross-backend relationship resolution (git-ref docs linking to filesystem/github-issues docs and vice versa)
- Fallback to reading refs directly via `GitRefOps` when cache is cold

### Out of Scope

- `GitRefOps` trait definition and `GitCli` implementation (Story 1)
- Lease engine: claim/release/heartbeat (Story 1)
- Agent identity (Story 1)
- `lazyspec init`/`lazyspec setup` wizard (Story 3)
- Fetch refspec management in `.git/config` (Story 3)
- TUI integration for git-ref documents (Story 4)
- Claude Code hooks for coordination (Story 5)
