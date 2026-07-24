---
title: Relationship-driven GitHub-native sub-issues
type: story
status: in-progress
author: unknown
date: 2026-07-24
tags: []
related:
- implements: RFC-050
---

## Problem

GitHub native sub-issue edges bind only to subdir structure: `index.md` parent + sibling child `.md` files (`gh_subissue.rs` reconciles from directory layout; `ops/create.rs` nests at create time). A flat two-type model — e.g. `feature` docs and `story` docs, every story a sub-issue of its feature — cannot produce native sub-issue edges at all. `github_native = "sub-issue"` parses and round-trips through config, but no link path consumes it: `relationship_by_github_native` is only ever called with `"milestone"` and `"dependency"`.

Result: teams organizing github-issues docs by relationship (`STORY implements FEATURE`) get no native parent/child nesting on GitHub — the hierarchy the board, tracking UI, and progress bars read from stays empty. STORY-244 already made this move for `blocks` → native dependencies; this story extends the same opportunistic-native pattern to sub-issues.

## Goal

Any `[[relationships]]` entry may declare `github_native = "sub-issue"`. When both endpoints are same-repo github-issues docs, `lazyspec link CHILD <rel> PARENT` writes the native sub-issue edge (**source = child, target = parent**: `STORY-A implements FEATURE-A` → `addSubIssue(issueId: feature, subIssueId: story)`), `unlink` removes it, and `fetch` reads native sub-issue edges back as the configured relation. Everywhere else — filesystem docs, cross-store, cross-repo — the relation stays comment/graph-backed with no native call and no error. No new CLI command.

## Design

### Opportunistic native, mirroring STORY-244

Same model as `github_native = "dependency"`: universal semantic relation gains an optional native edge. Fires only when both endpoints resolve to github-issues docs (same repo under lazyspec's one-repo-per-store model); otherwise a silent comment/graph-backed no-op. Existing filesystem usage regression-free.

### Config

`github_native = "sub-issue"` on the chosen relationship (e.g. `implements`). Field and parse already exist. New config validation: **at most one relationship may declare `github_native = "sub-issue"`** — GitHub sub-issues are single-parent, two competing relations would fight over the edge.

### Write path — link / unlink

`apply_native_subissue` alongside `apply_native_dependency` (src/engine/ops/link.rs), wired into `link_inner` and the unlink path. Reuses `ADD_SUB_ISSUE_MUTATION` / `REMOVE_SUB_ISSUE_MUTATION` from src/engine/gh_subissue.rs via the existing `GhGraphql` seam (sub-issues are GraphQL-only; node ids from the issue map / cache). Returns `true` on a native call so the caller routes through `resync_after_native_edge`.

**Single-parent enforcement:** linking a child that already has a native parent fails with a clear error naming the existing parent — no silent reparent. Reparenting is `unlink` old, `link` new.

### Read path — fetch

Fetch already batch-resolves native sub-issue edges (`fetch_sub_issue_nodes_batch`, src/engine/issue_cache.rs) to materialize subdir nesting. Split that consumption by parent type:

- Parent doc's type has `subdirectory: true` → materialize as nested subdir docs (today's behavior, ITERATION-224).
- Otherwise, and a relationship declares `github_native = "sub-issue"` → inject the relation (child → parent via the configured name, inverse on the parent), mirroring dependency read-back.
- Neither → current behavior (nest), preserving back-compat for configs with no sub-issue relationship.

### Coexistence with subdir path

Subdir-structural reconcile (`reconcile_subissues` keyed off `materialize_subdir`) is untouched and remains the authority for `subdirectory: true` types. Relationship path handles flat docs only. The two writers are mutually exclusive by parent type, so no double-write.

### Surfaces

Engine owns dispatch + read-back; CLI `link`/`unlink` route through `link_inner` (no CLI surface change beyond behavior); TUI link/unlink and web view inherit the native edge through the same engine path. `show --json` / `status --json` reflect round-tripped relations with no output-shape change.

## Non-goals

- Reparenting semantics (`replaceParent`) — explicit unlink+link only.
- Cross-repo sub-issues — GitHub permits same-owner cross-repo, lazyspec's one-repo-per-store model does not; stays comment-backed.
- Changing subdir-structural sub-issue behavior — untouched.
- Native binding for other semantic relations — only the one configured relationship.
- Conflict detection on native writes — inherits last-write-wins + resync.
- New CLI command or flag.

## Acceptance criteria

- **Given** a relationship (e.g. `implements`) configured with `github_native = "sub-issue"` and two same-repo github-issues docs STORY-A, FEATURE-A, **when** `lazyspec link STORY-A implements FEATURE-A` runs, **then** STORY-A becomes a native sub-issue of FEATURE-A (`addSubIssue`), the relation is recorded, and affected issue caches resync.
- **Given** the same, **when** `lazyspec unlink STORY-A implements FEATURE-A` runs, **then** the native sub-issue edge is removed and the relation record dropped.
- **Given** STORY-A already a native sub-issue of FEATURE-A, **when** `lazyspec link STORY-A implements FEATURE-B` runs, **then** the command fails with an error naming FEATURE-A as the existing parent and no native mutation fires.
- **Given** a native sub-issue edge set on GitHub out-of-band between two flat (non-subdir) issue-docs, **when** `lazyspec fetch` runs, **then** it surfaces as the configured relation with correct direction (child holds the forward relation, parent the inverse) in `show --json` / `status --json`, not as subdir nesting.
- **Given** a subdir-type parent, **when** `fetch` runs, **then** sub-issues still materialize as nested subdir docs exactly as today.
- **Given** a link where either endpoint is a filesystem doc, cross-store, or cross-repo, **when** `link`/`unlink` runs, **then** no native call fires, the relation is recorded comment/graph-backed, no error.
- **Given** two relationships both declaring `github_native = "sub-issue"`, **when** config loads, **then** validation rejects it with a clear message.
- **Given** the native write path, **then** it is exercised through a fake at the `GhGraphql` seam (no live GitHub in tests), consistent with dependency/membership tests.
