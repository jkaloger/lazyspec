---
title: GitHub-native blocks write path via issue dependencies
type: iteration
status: in-progress
author: jkaloger
date: 2026-07-22
tags: []
related:
- implements: STORY-244
- blocks: ITERATION-346
---

## Objective

`blocks`/`blocked-by` write path gains opportunistic GitHub-native issue-dependency edge on `link`/`unlink`.

## Satisfies

STORY-244 AC1, AC2, AC4, AC5, AC6 (write side). AC3 deferred — see Out of scope.

## Context

- Story + ACs + design: [[STORY-244]] §Design (opportunistic native, write path, trait seam, same-repo constraint)
- Native-binding precedent: [[RFC-050]] (milestone/membership mechanism)
- Touch:
  - `.lazyspec.toml:138` — add `github_native = "dependency"` to `blocks` relationship entry
  - `src/engine/gh.rs` — extend REST seam w/ issue-dependency ops (add/remove blocked_by); pure argv builder tested like `build_set_milestone_args`; real impl shells `gh api`
  - `src/engine/ops/link.rs` — new `apply_native_dependency` mirroring `apply_native_milestone` (link.rs:183) / `apply_native_membership` (link.rs:240); wire into `link_inner` (link.rs:58) + unlink path (link.rs:448); reuse `resync_after_native_edge`

## Tasks

1. Config: add `github_native = "dependency"` to `blocks` in `.lazyspec.toml`. `github_native: Option<String>` parse already exists (used by milestone/membership) — no schema change.
2. `gh.rs`: extend REST trait seam w/ issue-dependency ops (add/remove blocked_by). Pure argv builder + unit test like `build_set_milestone_args`. Real impl shells `gh api` against `/repos/{owner}/{repo}/issues/{n}/dependencies/blocked_by` — verify exact endpoint shape during impl.
3. Test-first: fake at the REST seam (mirror `MockGhMilestoneClient`). Cases: both-issues-same-repo link → native add + resync; unlink → native remove; `github_native != "dependency"` or non-issue/cross-repo endpoint → early `false`.
4. `link.rs`: `apply_native_dependency` — returns `false` (no native call) unless relation `github_native == "dependency"` AND both endpoints same-repo github-issues docs; else resolve both issue numbers, call add (link) / remove (unlink), return `true` → caller fires `resync_after_native_edge`.
5. Wire into `link_inner` + unlink path. Opportunistic guard: fs / cross-store / cross-repo → no native call, no error, relation recorded comment/graph-backed as today.

## Out of scope

- `fetch` native read-back (AC3) → next iteration (blocked-by this one; needs the REST seam landed here).
- Native binding for `implements` / `related-to` — STORY-244 non-goal.
- Cross-repo native deps, conflict detection — STORY-244 non-goals.

## Principles/conventions

- Project convention: engine owns dispatch; CLI/TUI inherit through same path (no CLI surface change beyond behavior).
- Principle 4: fake only at the trait seam. `type-driven-design`, `testing` skills.

## Verification

- Filesystem-only `blocks` link/unlink: zero gh calls (regression, AC5).
- Both-issues same-repo: native add observed on fake, `resync_after_native_edge` fires (AC1); unlink removes (AC2).

