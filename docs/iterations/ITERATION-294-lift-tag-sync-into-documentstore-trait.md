---
title: "Lift tag sync into DocumentStore trait"
type: iteration
status: accepted
author: "agent"
date: 2026-07-14
tags:
- tags
- store
related:
- implements: STORY-207
---

## Changes

### Task 1: Add `sync_tags` to `DocumentStore` trait + fs/unavailable impls

ACs addressed: fs-unchanged, adding-a-backend-forces-a-tag-decision

**Files:**
- Modify: `src/engine/store_dispatch.rs`

Add trait method (`store_dispatch.rs:44`, after `set_provenance`):

```rust
fn sync_tags(
    &mut self,
    type_def: &TypeDef,
    doc_id: &str,
    add: &[String],
    remove: &[String],
) -> Result<()>;
```

Propagation only — CLI still owns local frontmatter rewrite.

Impls:
- `FilesystemStore::sync_tags` → `Ok(())`. Doc file = source of truth, already rewritten.
- `UnavailableStore::sync_tags` → `bail!("{}", self.message)`, matches its other methods.
- `GithubMilestonesStore` / `GithubProjectsStore::sync_tags` → `Ok(())`. Labels not a milestone/project concept (see Notes).

New method breaks every existing impl until filled → compiler enforces coverage (AC: adding-a-backend-forces-a-tag-decision).

### Task 2: Move github label push into `GithubIssuesStore::sync_tags`

ACs addressed: github-issues-unchanged, sync-routes-through-store

**Files:**
- Modify: `src/engine/store_dispatch.rs`
- Modify: `src/cli/tag.rs`

Move body of `push_tags_if_github_backed` (`cli/tag.rs:108`) into `GithubIssuesStore::sync_tags`. Logic:
1. Resolve repo from github config (store already holds it).
2. Look up issue number via `IssueMap` / existing `check_lock` path (reuse store's existing lookup, same as `update`).
3. `add`: `label_ensure(repo, tag, "", &deterministic_color(tag))` each, then `issue_edit(repo, num, None, None, add, &[])`.
4. `remove`: `issue_edit(repo, num, None, None, &[], remove)`.
5. `issue_cache.touch_lock(doc_id)`.

No optimistic `check_lock` gate (label add/remove is atomic, independent of body) — preserve current skip, note it.

Delete `push_tags_if_github_backed`, `TagOp`, and the `GhCli`/`client_factory` generic plumbing from `cli/tag.rs`. No `StoreBackend::GithubIssues` comparison remains in the CLI (AC: sync-routes-through-store).

### Task 3: `GitRefStore::sync_tags` re-pushes ref

ACs addressed: git-ref-add-repushes, git-ref-remove-repushes, git-ref-no-coordination-no-push

**Files:**
- Modify: `src/engine/git_ref_store.rs`

CLI rewrites the cache file first (git-ref docs live in `.lazyspec/cache/<type>/`). `sync_tags` re-serializes current cache content into the ref blob + pushes. Reuse the ref-push tail of `update` (`git_ref_store.rs:201-238`): read cache file → `create_commit("doc.md", cache_content, old_sha)` → `update_ref` → `push_ref` only `if config.coordination.is_some()` → write lock.

Do NOT reuse `update`'s frontmatter mutation loop — it only rewrites single-line `key: value` fields, can't touch a `tags:` sequence block. Cache content is already correct; just persist + push it.

`add`/`remove` args unused by git-ref (cache already reflects them) — bind `_add`/`_remove` or assert non-empty for a clear error.

### Task 4: `ClickupTasksStore::sync_tags` fails loud

ACs addressed: clickup-fails-loudly

**Files:**
- Modify: `src/engine/store_dispatch.rs`

`bail!("{}", Self::WRITE_UNIMPLEMENTED)` (`store_dispatch.rs:111`). Explicit "not implemented" at trait seam, not silent `Ok`. ClickUp tag API + `ClickupClient` trait methods deferred to RFC-056 write path.

### Task 5: Route `cli/tag.rs` through store dispatch

ACs addressed: fs-unchanged, github-issues-unchanged, json-output, sync-routes-through-store

**Files:**
- Modify: `src/cli/tag.rs`
- Modify: `src/main.rs`

`tag_add`/`tag_remove` flow becomes: `resolve_to_path` → `rewrite_frontmatter` (local, backend-agnostic) → `store.sync_tags(type_def, doc_id, add, remove)`. `add` empties `remove` slice and vice versa. Load `type_def` from config by resolved path/id.

`main.rs`: dispatch already loads store for `Update` (`main.rs:150-181`); reuse `&mut Store` here. JSON reload/print path unchanged.

### Task 6: README + convention check

ACs addressed: (project convention)

**Files:**
- Modify: `README.md` (only if tag backend coverage documented; note git-ref now syncs, clickup errors)

No CLI surface change → likely no README edit. Verify.

## Test Plan

All store-impl tests in `store_dispatch.rs` / `git_ref_store.rs` `#[cfg(test)]`; CLI routing tests stay in `cli/tag.rs`. Trait seam mocks per principle 4.

### Unit: FilesystemStore::sync_tags is no-op
Call with add/remove, assert `Ok(())`, no side effect.

### Unit: GithubIssuesStore::sync_tags add
`MockGhClient`. Add `["security"]` → assert `label_ensure("security")` + `issue_edit(labels_add=["security"])` + `touch_lock`.

### Unit: GithubIssuesStore::sync_tags remove
Remove `["security"]` → assert `issue_edit(labels_remove=["security"])`, no `label_ensure`.

### Unit: GitRefStore::sync_tags pushes ref when coordinated
Fake git seam (existing git-ref test harness). Cache file has `tags: [security]`. Call `sync_tags` → assert `create_commit` blob frontmatter contains `security`, `push_ref` called.

### Unit: GitRefStore::sync_tags no push without coordination
No `coordination` → assert `create_commit`/`update_ref` called, `push_ref` NOT called, lock updated.

### Unit: ClickupTasksStore::sync_tags bails
Assert `Err` containing "not implemented".

### Unit: cli tag routes through store (fs + github)
Port STORY-206 tag tests to the dispatch path. Assert fs doc frontmatter + github label push both still fire. Assert no `StoreBackend::GithubIssues` literal in `cli/tag.rs` (grep-level, or structurally by absence of the branch).

### Regression
Existing STORY-206 tests (`cli/tag.rs`) pass unchanged in behaviour after refactor.

## Notes

- git-ref: CLI already rewrites the cache file because git-ref docs materialize under `.lazyspec/cache/<type>/`; `sync_tags` only re-commits + pushes. Do not duplicate the frontmatter mutation.
- github-milestones/github-projects `sync_tags` = no-op `Ok(())`: labels are an issue concept, not milestone/project. STORY-207 out-of-scope confirms.
- Keep the "no optimistic lock for tags" posture (label ops are atomic) — carry the existing doc comment from `push_tags_if_github_backed` onto `GithubIssuesStore::sync_tags`.
- Single `sync_tags(add, remove)` over two methods: mirrors `issue_edit(labels_add, labels_remove)`, one dispatch, one lock touch. Per ADR-024.
