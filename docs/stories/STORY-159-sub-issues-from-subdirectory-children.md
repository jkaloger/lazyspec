---
title: Sub-issues from subdirectory children
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

The `github-issues` store has a silent-drop gap: subdirectory document types (`TypeDef.subdirectory = true`, e.g. `convention`) are a directory of an `index.md` parent plus sibling `.md` files, each its own typed child doc. The loader (`src/engine/store/loader.rs`) tracks `children_of`/`parent_of`, but the github store only materializes `index.md` — the children are silently dropped on sync, so they never reach GitHub at all.

RFC-050 introduces a native-binding layer over the github store. This slice closes the drop gap and binds the now-materialized structural children to GitHub's **native sub-issues**: the `index.md` becomes the parent issue and each child `.md` becomes a sub-issue via the GraphQL sub-issues API (`addSubIssue` / `removeSubIssue` / `reprioritizeSubIssue`, GA 2025-03-17 — the `sub_issues` preview header is no longer required). This is a native-backed relation declared in config as `github_native = "sub-issue"`.

Sub-issue endpoints are issue-backed, so the parent and every child must live in the **same store**. GitHub itself permits same-owner cross-repo sub-issues; the same-store constraint is lazyspec's own, enforced as a guard. The flat parent→child shape this slice produces stays well within GitHub's ~100-children / 8-nesting limits.

Semantic relations (`implements`, `blocks`, `related-to`) are unaffected — they remain serialized in the issue-body HTML comment. Only structural subdirectory children map to native sub-issues. This slice depends on STORY-155 (the `GhGraphql` trait + schema snapshot).

## Acceptance Criteria

- **Given** a subdirectory document (an `index.md` with sibling child `.md` files) synced to the github store
  **When** the store materializes the doc
  **Then** the parent `index.md` materializes as an issue AND each child `.md` materializes as its own issue (regression coverage for the silent-drop gap; previously only the parent synced).

- **Given** a materialized subdirectory parent and a newly materialized child
  **When** the structural-children relation (`github_native = "sub-issue"`) is reconciled
  **Then** `addSubIssue` is called via `GhGraphql` linking the child issue to the parent issue, and the child appears under the parent's native sub-issues.

- **Given** a subdirectory parent with an existing native sub-issue link to a child
  **When** that child `.md` is removed from the subdirectory (or the structural relation no longer holds)
  **Then** `removeSubIssue` is called via `GhGraphql`, unlinking the child from the parent's native sub-issues.

- **Given** a sub-issue link whose proposed parent and child resolve to different stores
  **When** reconciliation attempts to bind them
  **Then** the same-store guard rejects the link with an error naming the offending parent/child and no `addSubIssue` mutation is issued.

- **Given** a parent with multiple children already linked as native sub-issues, in a child order derived from the loader (`children_of`, sorted by path)
  **When** the children's order is reconciled
  **Then** `reprioritizeSubIssue` is called via `GhGraphql` so the parent's native sub-issue order matches the loader order.

- **Given** a doc carrying both structural subdirectory children and a semantic `implements` relation
  **When** the doc is synced and re-read
  **Then** the structural children are bound as native sub-issues while the `implements` relation remains serialized in the issue-body HTML comment (unchanged, not promoted to a sub-issue).

## Scope

### In Scope

- Materialize subdirectory documents in the github store: the `index.md` parent issue plus each child `.md` as its own typed issue, using the loader's existing `children_of`/`parent_of` tracking.
- A `github_native = "sub-issue"` native-backed relation for structural subdirectory children, reconciled through `addSubIssue` / `removeSubIssue` (and `reprioritizeSubIssue` for ordering) over the STORY-155 `GhGraphql` trait.
- A same-store guard that rejects cross-store sub-issue links before any mutation, with a clear error.
- Fakes at the `GhGraphql` seam exercising add/remove/guard paths under TDD; every affected command keeps `--json` output.

### Out of Scope

- Mapping semantic relations (`implements`, `blocks`, `related-to`) to sub-issues — these stay comment-backed and unchanged (STORY out of RFC-050's relation work).
- Native issue-types, milestones, project membership, and per-board field attributes (separate RFC-050 stories).
- Cross-repo / cross-owner sub-issues, even though GitHub permits same-owner cross-repo ones; lazyspec stays same-store by construction.
- Conflict detection on native writes — last-write-wins per RFC-050.
