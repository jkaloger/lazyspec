---
title: Refresh milestone caches in TUI background poll
type: iteration
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-158
---## Root cause

TUI background poll refreshes `github-issues` types only. Milestone caches never refreshed live -> milestone created after last CLI `fetch` never appears in TUI.

- `event_loop.rs:525-530` poll thread: `gh_types = types.filter(|t| t.store == StoreBackend::GithubIssues)`. github-milestones excluded.
- `event_loop.rs:541` loop calls `store.issue_cache.fetch_all` per issue type only. No milestone fetch.
- TUI loads milestones from disk cache `.lazyspec/cache/<type>` at `Store::load` (`store.rs:64-70`). Live refresh = poll only. Poll skips milestones -> stale forever.
- CLI `fetch` DOES handle milestones: `fetch_milestones()` (`cli/fetch.rs:245`) run BEFORE issues (`cli/fetch.rs:73-90`). Asymmetry: poll replicates issues branch only.

Confirmed:
- `gh api repos/jkaloger/lazyspec/milestones/2` -> open, "Test milestone". `?state=all` lists both 1 + 2.
- disk cache had MILESTONE-1.md only.
- `cargo run -- fetch --type milestone` -> `fetched 2, new 1` -> MILESTONE-2.md materialized + parses. fetch logic + parse OK; poll is fault.

## Fix

1. Lift `fetch_milestones` out of `cli/fetch.rs:245` into shared seam (engine, e.g. `engine/milestone_cache.rs` or fn beside `IssueCache`) so CLI + TUI poll call one impl. Make seam return warnings (`Vec<RefreshWarning>`) alongside its `TypeSummary` so poll can route them (see step 3); CLI keeps printing summary as today. `GhCli` already impl `GhMilestoneApi` (`gh.rs:882`); poll already builds `GhCli::new()` + holds `store.repo`.

2. `event_loop.rs` poll thread: collect `github-milestones` types; fetch them BEFORE the issue loop. Ordering matters — issue `targets: MILESTONE-n` resolves milestone number through `issue_map`; milestone must map first or relation drops on fresh poll (same reason as `cli/fetch.rs:69-72`). Save `issue_map` after (poll already saves at `:563`).

3. Route milestone fetch warnings into the SAME warnings vec already sent on `AppEvent::CacheRefresh` (STORY-163 path) -> warnings panel. No `eprintln!` while alt screen active.

## Acceptance criteria

- Given github-milestones type + milestone created on GitHub after TUI launch, when poll TTL elapses, then milestone doc materializes in `.lazyspec/cache/<type>` and shows in TUI list — no restart, no manual CLI `fetch`.
- Milestone-before-issue order preserved: issue `targets: MILESTONE-n` relation resolves on a fresh poll (no dropped relation).
- `fetch_milestones` single source — no duplicated milestone-fetch body across `cli/fetch.rs` + `event_loop.rs`.
- Milestone fetch errors surface as `RefreshWarning` in warnings panel; zero stderr bytes while alternate screen active.
- Existing issue poll behaviour + CLI `fetch` output unchanged (`--json` unaffected).

## Out of scope

- github-projects live poll refresh. No project fetch path exists today (`cli/fetch.rs` fetches issues/milestones/git-ref only; board docs materialized at `create`, ITER-219). Same poll gap, but no path to mirror -> separate iteration under STORY-161/164.
- git-ref live poll refresh (materialized at `Store::load`; separate).
- Disk-cache startup load path unchanged.
- Milestone REST client / CRUD unchanged.
