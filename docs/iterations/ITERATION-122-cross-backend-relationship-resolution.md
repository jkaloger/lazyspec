---
title: Cross-backend relationship resolution
type: iteration
status: draft
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-099
---


## Changes

### Task 1: Add backend metadata to TypeDef and DocMeta

ACs addressed: unified-document-index, cross-backend-relationship-resolution

Files:
- Modify: `src/engine/config.rs` (add `store` field to `TypeDef`)
- Modify: `src/engine/document.rs` (add `backend` field to `DocMeta`)
- Modify: `src/engine/store/loader.rs` (propagate backend from TypeDef to DocMeta during load)

`TypeDef` needs a `store` field (`filesystem`, `git-ref`, `github-issues`) so the engine knows which backend owns each document type. `DocMeta` needs a `backend: Option<String>` field populated at load time from the owning TypeDef's store value.

Default to `"filesystem"` when unset, preserving backward compatibility with existing configs.

How to verify:
```
cargo test store
cargo test document
```

---

### Task 2: Build a unified document index across backends

ACs addressed: unified-document-index, cross-backend-relationship-resolution

Files:
- Modify: `src/engine/store.rs` (`Store::load_with_fs`)
- Create: `src/engine/store/backend.rs` (backend trait or dispatch logic)
- Modify: `src/engine/store/loader.rs` (dispatch per backend type)

`Store::load_with_fs` currently iterates `config.documents.types` and loads every type directory from the filesystem. Change this to dispatch per TypeDef based on its `store` field. For `filesystem` and `git-ref`, use the existing loader. For `github-issues`, read from the cache directory (`.lazyspec/cache/{type}/`).

The key invariant: after `Store::load`, `store.docs` contains all documents from all configured backends, keyed by their canonical path (or a synthetic path for non-filesystem documents like `github-issues:iterations/ITERATION-042`). The `id_to_path` map used by `build_links` and `resolve_target` then spans all backends.

How to verify:
```
cargo test store
```

---

### Task 3: Make resolve_target and resolve_shorthand backend-agnostic

ACs addressed: cross-backend-relationship-resolution

Files:
- Modify: `src/engine/store/links.rs` (`resolve_target`)
- Modify: `src/engine/store.rs` (`resolve_shorthand`, `resolve_unqualified`)

`resolve_target` already uses an `id_to_path` map built from `store.docs`. If Task 2 ensures all backend documents are in `store.docs`, resolution is automatically cross-backend. Verify this is the case and add guard logic for synthetic paths from non-filesystem backends.

`resolve_shorthand` uses `canonical_name` which extracts from filesystem paths. For github-issues documents with synthetic paths, the canonical name derivation may fail. Add a fallback that matches on the document's `id` field directly.

How to verify:
```
cargo test store
cargo test links
```

---

### Task 4: Add backend annotation to context chain output

ACs addressed: context-follows-cross-backend-chains

Files:
- Modify: `src/cli/context.rs` (`mini_card`, `run_human`, `run_json`)
- Modify: `src/cli/json.rs` (`doc_to_json_with_family`)

In `run_human`, the `mini_card` function renders a box for each chain document. Add a line showing the backend type (e.g. `[filesystem]`, `[github-issues]`). For github-issues documents, also show the issue number if available.

In `run_json`, add a `"backend"` field to each document in the chain output. The JSON format from `doc_to_json_with_family` should include the backend metadata from `DocMeta`.

The output should match the format sketched in RFC-037:
```
RFC-030 (Git-Based Document Number Reservation)  [filesystem]
  |
  STORY-075 (Auth refactor)                       [filesystem]
    |
    ITERATION-042 (Implementation)                [github-issues]
```

How to verify:
```
cargo test cli_context
```

---

### Task 5: Cross-backend broken link validation

ACs addressed: validate-detects-broken-cross-backend-relationships

Files:
- Modify: `src/engine/validation.rs` (`BrokenLinkRule`)

`BrokenLinkRule::check` builds an `id_to_path` map from `store.docs`. If the unified index (Task 2) includes all backends, broken link detection already works cross-backend: an iteration in github-issues referencing `STORY-075` in filesystem will resolve if both are in the index.

The change here is to improve the error message. When a broken link is detected, include the source document's backend and the unresolved target. This helps users understand whether the break is due to a missing document, a cache staleness issue, or a misconfigured backend.

Add a new validation check: if a document's relationship target resolves to a document in a backend that is not configured (e.g., referencing a github-issues document when no github-issues types are configured), emit a warning.

How to verify:
```
cargo test validation
cargo test cli_validate
```

---

### Task 6: Show command relationship expansion across backends

ACs addressed: show-expanded-relationships

Files:
- Modify: `src/cli/show.rs`
- Modify: `src/cli/json.rs`

The `show` command with relationship expansion calls `store.forward_links_for` and `store.reverse_links_for`, which return paths. These paths are then resolved via `store.get()`. If the unified index includes all backends, expansion already works cross-backend.

Add backend annotation to the expanded relationship output, so users can see which backend each linked document comes from. In JSON mode, include a `"backend"` field on each expanded relationship entry.

How to verify:
```
cargo test cli_show
cargo test cli_json
```

---

### Task 7: Integration test for cross-backend relationship chain

ACs addressed: all ACs

Files:
- Create: `tests/cross_backend_test.rs`

Build an integration test that sets up documents across simulated backends. Use the in-memory filesystem pattern from `src/engine/store.rs` tests. Create:

- An RFC in filesystem backend
- A Story in filesystem backend implementing the RFC
- An Iteration in a simulated github-issues backend (loaded from cache directory) implementing the Story

Assert:
1. `store.docs` contains all three documents
2. `resolve_shorthand` finds each document regardless of backend
3. `forward_links` / `reverse_links` cross backend boundaries
4. `resolve_chain` produces the full RFC -> Story -> Iteration chain
5. `validate_full` reports no broken links
6. Removing the iteration from the index and re-validating produces a broken link on the Story's reverse reference

How to verify:
```
cargo test cross_backend
```

## Test Plan

### Test 1: Unified index loads documents from all backends (AC: unified-document-index)
Configure three document types with different `store` values. Load the store. Assert `store.docs` contains documents from all three backends, each with the correct `backend` field set.

### Test 2: ID resolution crosses backend boundaries (AC: cross-backend-relationship-resolution)
Create a github-issues iteration with `implements: STORY-001`. Load the store. Assert `store.forward_links_for` on the iteration path returns a link pointing at the Story's path, and `store.reverse_links_for` on the Story path returns the iteration.

### Test 3: Context chain renders backend annotations (AC: context-follows-cross-backend-chains)
Build a three-document chain (RFC -> Story -> Iteration) across backends. Run `run_json`. Assert each entry in the `"chain"` array has a `"backend"` field with the correct value.

### Test 4: Context chain human output shows backend type (AC: context-follows-cross-backend-chains)
Same setup as Test 3. Run `run_human`. Assert the output string contains `[filesystem]` and `[github-issues]` annotations on the relevant documents.

### Test 5: Broken cross-backend link detected (AC: validate-detects-broken-cross-backend-relationships)
Create a document with `implements: ITERATION-999` where no such iteration exists in any backend. Run `validate_full`. Assert a `BrokenLink` error is emitted with the unresolved ID.

### Test 6: Valid cross-backend link passes validation (AC: validate-detects-broken-cross-backend-relationships)
Create a filesystem Story and a github-issues Iteration that implements it. Run `validate_full`. Assert no `BrokenLink` errors.

### Test 7: Show command expands cross-backend relationships (AC: show-expanded-relationships)
Create linked documents across backends. Run `show` with expansion. Assert the output includes the linked document from the other backend with its backend annotation.

### Test 8: Shorthand resolution for non-filesystem documents (AC: cross-backend-relationship-resolution)
Create a github-issues document with a synthetic path. Call `resolve_shorthand("ITERATION-042")`. Assert it returns the correct document.

## Notes

The Store is already close to backend-agnostic. Documents are keyed by `PathBuf` and relationships use ID-based targets (since ITERATION-117). The main gap is that `Store::load` only knows about the filesystem loader, and `DocMeta` carries no backend provenance.

For github-issues documents, the "path" in `store.docs` will be a synthetic path derived from the cache location. This is consistent with how git-ref documents work (they're also read from cache). The key constraint is that these synthetic paths must be stable across loads so that the link graph remains consistent.

The `context` command's chain-walking logic in `resolve_chain` follows `implements` links through `store.get`. Since it operates on the unified `store.docs` map, it already crosses backends once the index is unified. The rendering changes (backend annotations) are purely presentational.
