---
title: STORY-095 Issue CRUD and store dispatch
type: iteration
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/stories/STORY-095-issue-crud-and-store-dispatch.md
---


## Context

Wire up CRUD operations for `store = "github-issues"` document types. When a TypeDef
has this store value, create/update/delete route through the `gh` CLI integration layer
(STORY-094) instead of filesystem operations. This iteration adds the store field to
TypeDef, the dispatch logic in each CLI command, the issue-map for ID-to-number tracking,
optimistic locking on writes, and status-to-open/closed mapping.

Assumes STORY-094 (gh CLI integration layer) is complete and provides functions for
`gh issue create`, `gh issue edit`, `gh issue close`, `gh issue reopen`, and
`gh issue view` with JSON parsing.

## Changes

### 1. Add `store` field to TypeDef

Add an optional `store` enum field to `TypeDef` in `src/engine/config.rs`.
Values: `filesystem` (default), `github-issues`. Parse from `.lazyspec.toml`.

Files: `src/engine/config.rs`

### 2. Add `[github]` config section

Add a `GithubConfig` struct with `repo: Option<String>` and `cache_ttl` fields.
Parse `[github]` from `.lazyspec.toml`. Validate that `repo` is set when any type
uses `store = "github-issues"`.

Files: `src/engine/config.rs`

### 3. Issue map module

Create `src/engine/issue_map.rs` with:
- `IssueMapEntry { issue_number: u64, updated_at: String }`
- `IssueMap` backed by a `HashMap<String, IssueMapEntry>`
- `load(root)` reads `.lazyspec/issue-map.json`, returns empty map if absent
- `save(root)` writes the map back to disk
- `insert(id, number, updated_at)` and `get(id)` accessors

Files: `src/engine/issue_map.rs`, `src/engine.rs` (add module)

### 4. Store dispatch trait

Define a `StoreBackend` trait in `src/engine/store_dispatch.rs`:
- `create(type_def, title, author, body) -> Result<CreatedDoc>`
- `update(type_def, doc_id, updates) -> Result<()>`
- `delete(type_def, doc_id) -> Result<()>`

Implement `FilesystemBackend` that wraps the existing logic from
`src/cli/create.rs`, `src/cli/update.rs`, `src/cli/delete.rs`.

Implement `GithubIssuesBackend` that delegates to the gh integration layer.

Files: `src/engine/store_dispatch.rs`, `src/engine.rs`

### 5. GithubIssuesBackend::create

When `lazyspec create` targets a github-issues type:
1. Build the issue body (HTML comment frontmatter + markdown body)
2. Call `gh issue create` with title, body, and `lazyspec:{type}` label
3. Parse the returned issue number and `updated_at` from JSON response
4. Insert into issue-map and save
5. Write the cache file to `.lazyspec/cache/{type}/{id}.md`
6. Return the created doc path (cache path)

Files: `src/engine/store_dispatch.rs`, `src/cli/create.rs`

### 6. GithubIssuesBackend::update with optimistic locking

When `lazyspec update` targets a github-issues document:
1. Load issue-map, look up the issue number and stored `updated_at`
2. Call `gh issue view {number} --json updatedAt` to get current remote timestamp
3. Compare timestamps; if mismatch, return error with both timestamps
4. Build updated body/labels from the update fields
5. Call `gh issue edit {number}` with new body/labels
6. If status changed, call `gh issue close` or `gh issue reopen` per mapping
7. Update issue-map with new `updated_at`, save

Files: `src/engine/store_dispatch.rs`, `src/cli/update.rs`

### 7. GithubIssuesBackend::delete

When `lazyspec delete` targets a github-issues document:
1. Load issue-map, look up the issue number
2. Optimistic lock check (same as update)
3. Call `gh issue edit {number}` to prepend `[DELETED]` to title
4. Call `gh issue edit {number} --remove-label lazyspec:{type}`
5. Call `gh issue close {number}`
6. Remove entry from issue-map, save
7. Delete cache file if present

Files: `src/engine/store_dispatch.rs`, `src/cli/delete.rs`

### 8. Status mapping on writes

Implement the status-to-state mapping from RFC-037:
- `draft`, `review`, `accepted`, `in-progress` -> issue open
- `complete` -> issue closed (no frontmatter status)
- `rejected`, `superseded` -> issue closed (frontmatter status set)

When `lazyspec update --set-status` is called on a github-issues doc, apply the
mapping and call `gh issue close` or `gh issue reopen` as needed.

Files: `src/engine/store_dispatch.rs`

### 9. Route CLI commands through dispatch

Modify `src/cli/create.rs`, `src/cli/update.rs`, `src/cli/delete.rs` to:
1. Look up the TypeDef for the target document
2. Check `type_def.store`
3. If `github-issues`, delegate to `GithubIssuesBackend`
4. If `filesystem` (or absent), use existing filesystem logic

The existing filesystem code moves into `FilesystemBackend` with no behavior change.

Files: `src/cli/create.rs`, `src/cli/update.rs`, `src/cli/delete.rs`

## Test Plan

### Unit tests

- `TypeDef` parses `store = "github-issues"` and `store = "filesystem"` from TOML
- `TypeDef` defaults to `filesystem` when `store` is absent
- Config validation rejects github-issues types when `[github]` section is missing
- `IssueMap::load` returns empty map when file does not exist
- `IssueMap::save` then `load` round-trips correctly
- `IssueMap::insert` and `get` work as expected
- Status mapping returns correct open/closed state for each status value
- Optimistic lock comparison detects timestamp mismatch

### Integration tests

- `lazyspec create` with a github-issues type calls `gh issue create` and writes issue-map
- `lazyspec update` with matching timestamps succeeds and updates issue-map
- `lazyspec update` with mismatched timestamps fails with descriptive error
- `lazyspec delete` closes issue, removes label, prepends `[DELETED]`, cleans issue-map
- `lazyspec update --set-status complete` closes the issue
- `lazyspec update --set-status draft` reopens the issue
- Filesystem types are unaffected by the dispatch changes (regression)

### Manual verification

- Configure a type with `store = "github-issues"` in `.lazyspec.toml`
- Run `lazyspec create iteration "test doc"` and verify issue appears on GitHub
- Edit the issue on GitHub, then run `lazyspec update` and verify lock rejection
- Run `lazyspec fetch`, then retry update
- Run `lazyspec delete` and verify issue is closed with `[DELETED]` prefix

## Notes

The `gh` CLI must be installed and authenticated (`gh auth login`) for all
github-issues operations. The integration tests should mock or stub the gh
calls to avoid requiring a live GitHub connection in CI.
