---
title: Sub-issues from subdirectory children
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-159
---## Changes

### 1. Close subdir silent-drop gap in github store materialize

Today loader (`src/engine/store/loader.rs::load_subdirectory`) tracks `parent_of`/`children` for subdir types (`TypeDef.subdirectory = true`), but `GithubIssuesStore` (`src/engine/store_dispatch.rs:143`) never walks them -> only `index.md` becomes an issue, child `.md` dropped before reaching GitHub.

- `src/engine/store_dispatch.rs` -> `impl<G> GithubIssuesStore<G>`: add `fn materialize_subdir(&mut self, type_def, parent_doc_id) -> Result<MaterializeResult>`. Loads `Store` from `self.root`/`self.config` -> resolves parent `index.md` path -> `store.children_of(&parent_path)` (path-sorted = loader order). For parent + each child not yet in `self.issue_map`: run the existing create steps (label_ensure -> issue_create -> issue_map.insert -> write_cache_file). Parent issue = the `index.md` doc's issue. Load-vs-create ordering dependency: `children_of` only returns children already on disk -> in the `create` path the placeholder `index.md` is fresh, so impl must confirm children present before mapping; cache-refresh wiring (Changes §4) covers the post-settle case. Returns `{ parent_issue: u64, parent_node: String, children: Vec<(child_id, issue_number, node_id, order_index)> }`.
- `GithubIssuesStore::create` (line 217): when `type_def.subdirectory`, after creating the `index.md` parent issue, call `materialize_subdir` so children co-materialize (flat parent->children, no recursion for the 1-level subdir shape).
- `IssueMap`/`IssueMapEntry` (`src/engine/issue_map.rs:10`): add `node_id: String` field; `insert` gains a `node_id` param (alongside existing `updated_at`). GraphQL `addSubIssue`/`removeSubIssue`/`reprioritizeSubIssue` key off issue **node ids** (`I_*`), NOT REST numbers -> capture node id at create/fetch. `GhIssue` (`src/engine/gh.rs:22`) gains `#[serde(default)] id: String` (GraphQL node id via `--json id`); fetch field lists in `issue_list`/`issue_view` gain `id`.

### 2. github_native sub-issue relation config + reconcile

- `src/engine/config.rs::RelationshipDef` (line 256): add `#[serde(default, skip_serializing_if = "Option::is_none")] github_native: Option<String>` (`"sub-issue"`). `config_write.rs` writer emits it. Structural subdir children declare `github_native = "sub-issue"`.
- New `src/engine/gh_subissue.rs`: `fn reconcile_subissues(gql: &dyn GhGraphql, repo, plan: &SubIssuePlan) -> Result<()>`. `SubIssuePlan` built from `materialize_subdir` result + current remote sub-issue set (read via GraphQL `issue.subIssues` query through `GhGraphql`). Diff:
  - desired child not yet linked -> `addSubIssue(issueId: parent_node, subIssueId: child_node)`.
  - remote sub-issue no longer a structural child (child `.md` removed / relation gone) -> `removeSubIssue(issueId: parent_node, subIssueId: child_node)`.
  - order mismatch vs loader order -> `reprioritizeSubIssue(issueId: parent_node, subIssueId: child_node, afterId: prev_child_node)` per out-of-place child.
- `GhGraphql` trait + `GqlVar` (from STORY-155 / ITERATION-210, `src/engine/gh.rs`): mutations via `gql.graphql(MUTATION, &[("issueId", GqlVar::Str(parent_node)), ("subIssueId", GqlVar::Str(child_node))])`. Node ids passed as `-f` string vars.

### 3. Same-store guard

- `gh_subissue::reconcile_subissues`: before any mutation, resolve parent + each child to their `StoreBackend` via config type lookup. If parent store != child store -> `bail!("sub-issue link rejected: parent {parent_id} (store {a}) and child {child_id} (store {b}) are in different stores; lazyspec sub-issues are same-store only")`. Zero `addSubIssue` issued. (GitHub permits same-owner cross-repo; constraint is lazyspec's.)

### 4. Wiring

- `materialize_subdir` -> build `SubIssuePlan` -> `reconcile_subissues`. Called from `create` (subdir path) and from cache refresh of subdir types so add/remove/reprioritize run after children settle.
- Semantic relations (`implements`/`blocks`/`related-to`, `github_native = None`) untouched: stay in issue-body HTML comment via `issue_body::serialize` (`src/engine/issue_body.rs:25`). Only `github_native = "sub-issue"` relations route to GraphQL.
- `--json` preserved on every touched command (`create`, `link`, `fetch`).

## Test Plan

AC1 (subdir children materialize; regression for drop gap): `store_dispatch.rs` test -> `MockGhClient`, subdir `type_def` (`subdirectory = true`), fixture `index.md` + 2 child `.md`. Call `materialize_subdir` (or `create`) -> assert `issue_map` holds parent + both child issue numbers AND `MockGhClient` recorded 3 `issue_create` calls (pre-fix: 1).

AC2 (child->parent native sub-issue via addSubIssue): fake `GhGraphql` recording mutations. Materialize subdir -> `reconcile_subissues` -> assert `addSubIssue` called once per child with `issueId = parent_node`, `subIssueId = child_node`.

AC3 (removeSubIssue on unlink): fake `GhGraphql` whose `subIssues` query returns a child already linked. Remove that child `.md` from fixture (structural relation no longer holds) -> reconcile -> assert `removeSubIssue` called with that child node, no spurious `addSubIssue`.

AC4 (same-store guard): build plan where a child resolves to a different `StoreBackend` than parent -> `reconcile_subissues` returns `Err` naming offending parent + child; fake `GhGraphql` records zero `addSubIssue`.

AC5 (ordering via reprioritize): fake returns children linked in wrong order vs loader (`children_of` path-sorted) -> reconcile -> assert `reprioritizeSubIssue` called so remote order matches loader order; assert no add/remove when only order differs.

AC6 (structural native + semantic comment coexist): subdir doc with children AND an `implements` relation. Sync -> assert children -> `addSubIssue` (GraphQL) while `implements` appears in serialized issue-body HTML comment (`issue_body::serialize` output) and is NOT sent to GraphQL.

## Notes

- Sub-issues GA 2025-03-17: `sub_issues` preview Accept header NO longer required; plain `gh api graphql` works.
- Mutations key off issue **node ids** (`I_*`), not REST numbers -> `IssueMap` must persist `node_id`; `GhIssue`/fetch must request `id`.
- Same-store constraint is lazyspec's, NOT GitHub's (GitHub allows same-owner cross-repo sub-issues). Enforced as guard.
- Flat parent->child shape stays within GitHub limits (~100 children / 8 nesting) -> no batching/recursion concern.
- Depends on STORY-155 / ITERATION-210 `GhGraphql` trait + `GqlVar` seam; fakes live at same seam as `MockGhClient`.
- Semantic relations stay comment-backed; only `github_native = "sub-issue"` is native. Last-write-wins per RFC-050 (no conflict detection on the mutation).