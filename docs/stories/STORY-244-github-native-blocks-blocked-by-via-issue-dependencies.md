---
title: GitHub-native blocks/blocked-by via issue dependencies
type: story
status: in-progress
author: jkaloger
date: 2026-07-22
tags: []
related:
- implements: RFC-050
---## Problem

lazyspec's `blocks`/`blocked-by` relation (`.lazyspec` config `[[relationships]]`) is purely comment-backed for github-issues docs: the edge lives in the issue-body HTML/YAML relations block, invisible to GitHub itself. RFC-050 built the `github_native` binding mechanism for relations — `sub-issue` (`addSubIssue`/`removeSubIssue`) and `membership` (`addProjectV2ItemById`), dispatched from `apply_native_membership` (src/engine/ops/link.rs:240) and read back on fetch (src/engine/sync.rs:737) — but explicitly scoped semantic relations OUT:

> "Semantic relations (`implements`, `blocks`, `related-to`) stay comment-backed and unchanged."  — RFC-050, Design §2

Since then GitHub shipped **native issue dependencies** (blocked-by / blocking), reachable through `gh api`. Two lazyspec issue-docs that block each other therefore have their dependency recorded only in a body comment while GitHub's own dependency graph — the one the team's board, notifications, and "blocked" badges read from — stays empty. A doc backed by a GitHub issue cannot answer "what issues block this" from GitHub-native state, and a dependency set on GitHub is invisible to lazyspec.

This story reverses RFC-050's blocks-stays-comment-backed line for the github-issues store. Recorded here rather than by amending accepted RFC-050; RFC-050's decision reads stale and this story is the supersession record.

## Goal

`blocks`/`blocked-by` gains a `github_native` binding to GitHub's issue-dependencies API. When both endpoints are github-issues docs in the same repo, `lazyspec link A blocks B` / `unlink` writes the native GitHub dependency edge (in addition to its normal relation record), and `fetch` reads native dependencies back into the graph as `blocks`/`blocked-by`. Everywhere else — filesystem docs, cross-store, cross-repo — the relation stays comment/graph-backed exactly as today. All through the existing lazyspec CLI; no new command.

## Design

### Opportunistic native, not native-only

This is the key departure from `sub-issue`/`membership`, which are native-*only* (they reject non-issue endpoints). `blocks` is a **universal semantic relation that gains an optional native edge**. The native GitHub dependency fires only when *both* source and target resolve to github-issues docs in the same repo; otherwise the relation is recorded comment/graph-backed with no native call and no error. Existing filesystem `blocks` usage is untouched (regression-free).

### Config

Add `github_native = "dependency"` to the `blocks` `[[relationships]]` entry (`.lazyspec`), parsed by the existing `github_native: Option<String>` field on the relationship config (src/engine/config.rs). No schema change — the field and its parse already exist for milestone/membership.

### Write path — link / unlink

Add an `apply_native_dependency` dispatch alongside `apply_native_milestone` (src/engine/ops/link.rs:183) and `apply_native_membership` (link.rs:240), wired into both `link_inner` (link.rs:99) and the unlink path (link.rs:448). It:
1. Returns early (no native call, `false`) unless the relation's `github_native == "dependency"` AND both endpoints are same-repo github-issues docs.
2. Resolves both issue numbers, then calls the GitHub issue-dependencies API (REST `dependencies/blocked_by` endpoints under `/repos/{owner}/{repo}/issues/{n}`, reachable via `gh api`, mirroring the milestone REST path — exact endpoint shape verified during the iteration). link adds the dependency, unlink removes it.
3. Returns `true` so the caller triggers `resync_after_native_edge` (link.rs:684), refreshing the affected issue caches — same push-first-then-resync discipline the milestone/membership paths use.

### Read path — fetch

On fetch, read each issue's native dependencies and inject them as `blocks`/`blocked-by` relations into the resolved graph, mirroring the milestone read-back (src/engine/sync.rs:558) and membership read-back (sync.rs:737). Direction: a native "blocked-by B" on issue A surfaces as `A blocked-by B` / `B blocks A`, consistent with the relation's declared inverse.

### Trait seam

Issue dependencies are REST, so they extend the existing `GhMilestoneApi`/REST reader-writer seam rather than `GhGraphql` (src/engine/gh.rs). Fakeable at the same seam as the other `Gh*` traits per convention (principle 4); real impl shells to `gh api`.

### Same-repo constraint

GitHub issue dependencies are within a single repo. lazyspec's one-repo-per-github-store model makes this the natural boundary — the same lazyspec-imposed constraint sub-issues rely on. Cross-repo `blocks` between issue-docs stays comment-backed (falls into the opportunistic "otherwise" branch), no error.

### Surfaces

Per CLAUDE.md, changes land across all three consumers. Engine owns dispatch + read-back; CLI `link`/`unlink` already route through `link_inner` (no CLI surface change beyond behavior); TUI link/unlink and web view inherit the native edge through the same engine path. `--json` on `show`/`status` reflects the round-tripped relations with no output-shape change.

## Non-goals

- Native binding for `implements` / `related-to` — RFC-050's other semantic relations stay comment-backed; only `blocks`/`blocked-by` moves.
- Cross-repo native dependencies — out of GitHub's and lazyspec's model; stays comment-backed.
- Conflict detection on native writes — inherits RFC-050's last-write-wins + resync policy.
- Amending accepted RFC-050 — the reversal is recorded in this story (see Problem).
- New CLI command or flag — reuses existing `link`/`unlink`.
- Offline validation of dependency targets beyond existing relation validation — no gh-schema snapshot entry needed (dependencies are issue refs, not a field-option set).

## Acceptance criteria

- **Given** `blocks` configured with `github_native = "dependency"` and two github-issues docs A, B in the same repo, **when** `lazyspec link A blocks B` runs, **then** GitHub's native issue-dependency edge (A blocking B / B blocked-by A) is created via the issue-dependencies API, the relation is recorded, and the affected issue caches resync.
- **Given** the same, **when** `lazyspec unlink A blocks B` runs, **then** the native dependency edge is removed and the relation record dropped.
- **Given** a native dependency set on GitHub out-of-band, **when** `lazyspec fetch` runs, **then** it surfaces as `blocks`/`blocked-by` in the graph and in `show --json` / `status --json`, with the correct inverse direction.
- **Given** a `blocks` link where either endpoint is a filesystem doc, cross-store, or cross-repo, **when** `link`/`unlink` runs, **then** no native API call fires, the relation is recorded comment/graph-backed exactly as today, and no error is raised.
- **Given** an existing filesystem-only project using `blocks`, **when** this ships, **then** behavior is byte-for-byte unchanged (regression-free).
- **Given** the native write path, **then** it is exercised through a fake at the REST trait seam (no live GitHub in tests), consistent with the milestone/membership tests.