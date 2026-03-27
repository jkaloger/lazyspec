---
title: Cross-backend document resolution
type: audit
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- related-to: RFC-037
- related-to: STORY-099
---



## Scope

Architectural audit of document ID resolution across store backends. Triggered by `lazyspec update TESTGH-001 --body "hi"` returning "document not found" despite the document existing in GitHub and the issue map. The question: can every CLI command that accepts a document ID resolve it regardless of which backend stores it?

## Criteria

RFC-037 states that any document type can use `store = "github-issues"` and that CLI commands (`show`, `update`, `delete`, `context`, `link`, etc.) should work transparently across backends. The `Store` resolution layer should route to the correct backend without the caller needing to know where a document lives.

## Findings

### Finding 1: `resolve_shorthand_or_path` only searches filesystem

- Severity: critical
- Location: `src/cli/resolve.rs:5-13`
- Description: `resolve_shorthand_or_path` searches only the filesystem `Store` (which indexes documents loaded from disk). It has no knowledge of the issue map or github-issues documents. Any command that resolves a document ID through this function will fail for github-issues documents with "document not found".
- Recommendation: The resolution layer needs a unified index that spans all backends. Either extend `Store` to include github-issues documents (loaded from cache + issue map), or introduce a multi-backend resolver that falls back through each backend in order.

### Finding 2: `Store` only indexes filesystem documents

- Severity: critical
- Location: `src/engine/store.rs:36-77`
- Description: `Store::load` iterates config document types and calls `load_type_directory` for each. This only reads `.md` files from the configured directories on disk. Documents with `store = "github-issues"` have no filesystem representation in the type directory (they live in `.lazyspec/cache/`). The `Store` is structurally incapable of indexing github-issues documents.
- Recommendation: Either teach `Store::load` to also load cached github-issues documents from `.lazyspec/cache/`, or separate the resolution concern from the storage concern so that resolution can query multiple sources.

### Finding 3: `IssueMap` stores no document metadata

- Severity: high
- Location: `src/engine/issue_map.rs:9-19`
- Description: `IssueMap` only tracks `doc_id -> (issue_number, updated_at)`. It does not store `doc_type`, `title`, `status`, or any other metadata needed for resolution. Even if the resolver checked the issue map, it couldn't return a `DocMeta` without fetching from the cache or GitHub.
- Recommendation: Either enrich the issue map with enough metadata for resolution (at minimum `doc_type`), or ensure the cache always has parseable documents that can be loaded into the `Store` index.

### Finding 4: `update` and `delete` have a partial workaround that doesn't generalize

- Severity: high
- Location: `src/cli/update.rs:24-41`, `src/cli/delete.rs:22-41`
- Description: `update` and `delete` resolve through the filesystem Store first, extract `doc_type`, then route to `GithubIssuesStore` if the type uses that backend. This works _only if the document also exists in the filesystem Store_. For github-issues-only documents (no filesystem representation), resolution fails at the first step. The workaround is coupled to the resolution succeeding, which defeats the purpose.
- Recommendation: These commands need a resolution path that doesn't depend on the filesystem Store. The `type_def` could be inferred from the document ID prefix (e.g. `TESTGH-001` -> type `testgh`) using the config's type definitions and prefix mappings.

### Finding 5: `show`, `link`, `pin`, `ignore` have no github-issues dispatch at all

- Severity: high
- Location: `src/cli/show.rs:23`, `src/cli/link.rs:9-10`, `src/cli/pin.rs:93`, `src/cli/ignore.rs:9`
- Description: These commands call `resolve_shorthand_or_path` and operate on the result as a filesystem document. They have no github-issues code path. Even if resolution were fixed, these commands would need dispatch logic to fetch content from GitHub (or cache) rather than reading from the filesystem.
- Recommendation: These need the same backend dispatch pattern that `create`, `update`, and `delete` have. A `DocumentStore::show` or `DocumentStore::read` method would unify this.

### Finding 6: Prefix-based type inference is available but unused

- Severity: info
- Location: `src/engine/config.rs` (type definitions with prefix field)
- Description: Each type definition in the config has a `prefix` field (e.g. `testgh` type has prefix `TESTGH`). Document IDs like `TESTGH-001` encode their type in the prefix. The resolution layer could parse the prefix from the ID and look up the type definition to determine which backend to use, without needing the filesystem Store at all.
- Recommendation: Use prefix-based type inference as the first step in a multi-backend resolver. Parse `TESTGH` from `TESTGH-001`, find the type with that prefix, check its `store` field, and route accordingly.

## Summary

The `Store` and resolution layer were designed for filesystem-only documents. Adding `store = "github-issues"` created a second backend, but the resolution layer was never updated to span both. The result: github-issues documents are invisible to most CLI commands.

The fix is architectural. The resolution layer needs to be backend-aware. The cleanest path is prefix-based type inference (finding 6) combined with loading cached github-issues documents into the Store index (finding 2). This would make resolution work without changing every command individually.
