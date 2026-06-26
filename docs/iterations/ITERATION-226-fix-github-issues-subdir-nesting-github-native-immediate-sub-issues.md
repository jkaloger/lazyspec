---
title: 'Fix github-issues subdir nesting: github-native immediate sub-issues'
type: iteration
status: draft
author: unknown
date: 2026-06-26
tags: []
related:
- implements: STORY-166
---

## Root cause

github-issues + subdirectory nesting broken. 3 components disagree where unpushed child lives.

- create.rs:244 create_with_parent -> writes child as local .md in .lazyspec/cache/<type>/<PARENT>/, never pushes to GitHub (early-return before github branch). No issue, no number.
- fetch.rs:274 subdir_parent_ids -> forces StoreBackend::Filesystem -> reads type_def.dir (docs/tickets, nonexistent) -> 0 parents -> reconcile_subdir_subissues no-op. github-issues docs live in .lazyspec/cache, not docs/<type>.
- issue_cache.rs:392 fetch_all -> wipes cache dir before reconcile runs -> any local child destroyed.

Net: child written to cache -> fetch wipes -> reconcile looks in wrong empty dir -> nothing materializes as sub-issue.

Confirmed: docs/tickets absent; gh graphql subIssues(65)=[]; cache flat.

Secondary: refusing cache write for empty doc id x2 from fetch.rs:203 inject_project_fields_into_cache. DocMeta::parse leaves id empty (document.rs:473); cache frontmatter carries no id:. write_cache_file rejects empty id -> project-field injection silently skipped every doc.

## Fix: github-native immediate model

1. create.rs create_with_parent: for github-issues store parent, create child as real GitHub issue (store.create) then addSubIssue(parent_node, child_node) via gh_subissue. Drop local-only cache .md path. Keep same-store guard. Resolve parent_node + child_node via issue_map.

2. Drop filesystem-source reconcile: remove StoreBackend::Filesystem override in subdir_parent_ids and the docs/<type> reconcile_subdir_subissues path. Children become sub-issues at create time; fetch mirrors nested via existing path (fetch_nested_subissues_test.rs covers nest-on-fetch).

3. Empty-doc-id: inject_project_fields_into_cache derive id from cache filename before write_cache_file (filename stem is canonical id for github-issues cache). Or fill meta.id from path. No empty-id write.

## Acceptance criteria

- create ticket --parent TICKET-65 -> GitHub issue created + sub-issue edge; gh api graphql subIssues(65) lists child.
- fetch -> nested cache .lazyspec/cache/ticket/TICKET-65/index.md + NN-child.md.
- No 'refusing cache write for empty doc id' warning; PROJECT-n fields injected.
- Cross-store parent/child still rejected (same-store guard).
- fetch_nested_subissues_test.rs still green; add test for create-time addSubIssue edge.

## Out of scope

- Filesystem-store subdirectory types unchanged (still author under type_def.dir).
- Fetch nesting read path unchanged.
