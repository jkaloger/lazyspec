---
title: 'GitHub deep-links: engine::github_url per backend + repo coordinates'
type: iteration
status: complete
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-180
---

## Objective

Add `engine::github_url(doc, repo_coords) -> Option<Url>` deriving each document's GitHub deep-link (blob / issue / milestone) from its store backend, plus `[web]`-override-then-`origin` repo-coordinate resolution, returning `None` (and a one-line `serve` startup warning) when coordinates can't resolve.

## Satisfies

STORY-180, all ACs. The `github_url` mapping, coordinate resolution, the `[web]` table, and graceful `None` are one tightly coupled unit and ship as a single slice. The page-rendering ACs (outbound "edit on GitHub" link) are verified in STORY-177's page render, not here — see Out of scope.

## Context

- Story + ACs (the authoritative behavior list): STORY-180.
- RFC sections: RFC-052 §"GitHub deep-links" (backend→URL mapping), §Interfaces (`engine::github_url`, `.lazyspec.toml` `[web]` table, `None` conditions).
- Backend the document came from: `StoreBackend` enum in `src/engine/config.rs`; the backend is on `TypeDef.store`, not on `DocMeta` — resolve it via the document's `doc_type` against `config.documents.types`. No backend field exists on the document itself.
- Existing coordinate resolution to reuse: `src/engine/github.rs` — `resolve_repo`, `infer_github_repo`, `parse_owner_repo` (`[web]` override slots in *ahead* of these; do not duplicate the `origin` parsing).
- Branch detection to reuse: `src/engine/git_status.rs` — `query_git_branch` (returns `None` on detached HEAD; that is one of the `None` paths).
- Issue/milestone native IDs: `src/engine/issue_map.rs` — `IssueMap`/`IssueMapEntry` (keyed by shorthand id, carries `issue_number` and `kind`); `GhIssue.url`/`GhMilestone.url` in `src/engine/gh.rs` are the already-resolved URLs for cached github-backed docs.
- Config surface: `GithubConfig` in `src/engine/config.rs` and `Config::parse`/`RawConfig` — the `[web]` table is the new optional sibling table.
- Touch: `src/engine/github.rs` or a new `src/engine/github_url.rs` (the function + a `RepoCoords` type + resolution), `src/engine/config.rs` (`WebConfig` struct + `RawConfig` wiring), `.lazyspec.toml` (none required, table is optional), the `serve` command (startup warning when coords unresolved — depends on STORY-176's skeleton).

## Backend→URL mapping (slice-specific, the spec gives the shapes only)

`RepoCoords` carries `owner`, `repo`, `branch`. Resolution order: `.lazyspec.toml` `[web]` (any of owner/repo/branch present overrides that field) → else `origin` remote (`resolve_repo`/`infer_github_repo` for owner/repo, `query_git_branch` for branch). If owner or repo cannot be obtained, coords are unresolved and `github_url` returns `None` for every document.

Given resolved coords, dispatch on the document's `StoreBackend`:

- `Filesystem` → `https://github.com/{owner}/{repo}/blob/{branch}/{path}` (path is `DocMeta.path` relative to repo root). No branch → `None`.
- `GithubIssues` → the issue URL: prefer the cached `GhIssue.url`; otherwise construct `…/issues/{n}` from the `IssueMap` `issue_number`. No map entry → `None`.
- `GithubMilestones` → the milestone URL: cached `GhMilestone.url`, else `…/milestone/{n}` from the map. No map entry → `None`.
- Any other backend (e.g. `GithubProjects`, `GitRef`) whose `github_native` mapping yields no stable single-document URL → `None`.

`None` is returned rather than a guessed/broken URL in every gap above. The function is pure over `(doc, RepoCoords)`; coordinate *resolution* (which touches git/config) is a separate function so the mapping is unit-testable without a git repo.

## Tasks

1. Add `WebConfig { owner, repo, branch }` (all optional) to `src/engine/config.rs`, wire it through `RawConfig` and `Config::parse` as the optional `[web]` table. Test that a `[web]` table parses and that its absence is fine.
2. Implement coordinate resolution returning `Option<RepoCoords>`: `[web]` override first, then `origin` (reuse `github.rs`/`git_status.rs`). Test the override-beats-`origin` precedence and the unresolved (no remote / detached HEAD) → `None` case.
3. Implement `engine::github_url(doc, coords)` dispatching on the document's resolved `StoreBackend` per the mapping above. Test-first: one case per backend (filesystem blob, github-issues, github-milestones) plus the `None` gaps (no branch, no issue-map entry, unsupported backend, unresolved coords).
4. In the `serve` command, resolve coords once at startup; on `None`, log a one-line warning that deep-links are disabled and continue serving (depends on STORY-176).
5. Export `github_url`/`RepoCoords` from the engine module and update the README if the `[web]` config table is user-facing.

## Out of scope

- The page-side "edit on GitHub" link rendering and its seam verification → STORY-177's document page.
- Editing / write-back of any kind — the link hands off to GitHub's own editor (RFC non-goal).
- Multi-remote selection beyond `origin`.
- OAuth, the membership gate, and hosted bind → STORY-181; deep-links work on the unauthenticated local view.
- The graph/search/skeleton routes (STORY-176/178/179) — unrelated to this slice.

## Principles / conventions

- `lazyspec` conventions: dev binary via `cargo run`; update the README when a user-facing config/CLI surface changes (per project CLAUDE.md).
- RFC-052 layering principle 3: `github_url` lives in `engine`, depends only on `engine`; the `serve` wiring is the only `web`/CLI touch and stays behind the `web` feature.
- Type-driven-design (skill): model `RepoCoords` and the backend dispatch so an unresolved coordinate set and a "no link for this backend" are distinct, non-confusable `None` paths rather than empty strings; return `Option<Url>`, never a stringly-typed maybe-URL.

## Verification

- A filesystem-backed doc with resolved coords yields `…/blob/{branch}/{path}`; the same doc with branch unresolved yields `None`.
- A `[web]` owner/repo/branch override is honoured over a differing `origin` remote.
- github-issues / github-milestones docs yield their issue / milestone URLs; a doc with no issue-map entry yields `None`.
- With no `origin` remote (or detached HEAD), `github_url` returns `None` for all docs and `serve` prints exactly one startup warning then serves.

