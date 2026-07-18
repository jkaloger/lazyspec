---
title: "GitHub milestone/issue sync emits canonical open/closed lifecycle"
type: iteration
status: complete
author: "unknown"
date: 2026-07-19
tags: []
related:
- implements: STORY-224
- related-to: STORY-223
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Objective

github-milestones/github-issues sync emits a canonical `open`/`closed`
`Lifecycle` when the type declares none, persisted like ClickUp. Undeclared
milestone/issue types then show `open`/`closed` status + DAG, not `draft`.

## Satisfies

STORY-224 AC1, AC2, AC3, AC5. AC4 already delivered (regression-guard only).
AC6 holds by construction (fs/git-ref emit no lifecycle).

## Context

- Story + ACs: STORY-224.
- Why not persist-on-sync (ClickUp pattern): ClickUp doc status is the *raw* remote
  status; GitHub status is *derived through* the lifecycle at cache-write time
  (`milestone_state_to_status`, store_dispatch.rs:1650; `issue_cache`
  first-active/terminal, issue_cache.rs:233). Persisting after the cache write
  yields `draft` on the first fetch — ordering bug. A resolver makes derivation
  correct on the first fetch with no config mutation.
- Readers of `type_def.lifecycle` to route through the resolver:
  `milestone_cache.rs:48`, `issue_cache.rs:233`, `open_status_picker`
  (tui/state/app.rs:2873), write-through terminal (store_dispatch.rs:1807),
  `open_status`/`closed_status` (store_dispatch.rs:685/1396/1557/1728),
  `update.rs:24` edge gate.

## Changes

1. `StoreBackend::canonical_lifecycle() -> Option<Lifecycle>` (config.rs):
   `GithubIssues`/`GithubMilestones` → `Some(Lifecycle{ states:["open","closed"],
   edges:[] })` (empty edges = unconstrained → bidirectional DAG + reopen;
   `first_active_status`="open", `terminal_status`="closed"); all others `None`.
2. `TypeDef::effective_lifecycle(&self) -> Cow<'_, Lifecycle>`: `Borrowed(&self.lifecycle)`
   when `states` non-empty (declared wins — AC3); else `store.canonical_lifecycle()`
   as `Owned`, falling back to `Borrowed(&self.lifecycle)` when `None` (AC6).
3. Route the readers above through `td.effective_lifecycle()`; TUI picker uses
   `type_def.effective_lifecycle()`.
4. README: note github-backed types get an `open`/`closed` DAG with no config
   declaration required.

## Test Plan

- AC1: engine — milestone type, empty declared lifecycle → `effective_lifecycle`
  states `[open,closed]`; `milestone_state_to_status("open")`=="open",
  `("closed")`=="closed"; `targets_from("open")` contains `closed` and vice-versa.
- AC2: same via issue cache derivation.
- AC3: type with declared non-empty lifecycle → `effective_lifecycle` == declared.
- AC5: `status_maps_to_open("open", terminal="closed")`==true → reopen (existing
  write-through test, now with resolved terminal).
- AC4 regression: ClickUp (persisted, non-empty) → `effective_lifecycle` == declared;
  existing derive/persist tests green.
- AC6: `filesystem`/`git-ref` empty lifecycle → `effective_lifecycle` stays empty.
- `cargo test`, `cargo clippy`.

## Notes

- Resolver, not persist: github status is lifecycle-derived, so the lifecycle must
  be correct at cache-write time (persist-after would need a second fetch).
- No `.lazyspec.toml` mutation for github — the DAG is resolved at runtime.
- Empty edges deliberate (GitHub owns transitions; lazyspec adds no gating) — same
  posture as ClickUp `derive_lifecycle`.
