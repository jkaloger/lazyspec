---
title: "Route fetch warnings to the TUI warnings panel"
type: iteration
status: draft
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: STORY-163
---
## Changes

### 1. Engine: drop the 3 `eprintln!`, return warnings as data (`src/engine/issue_cache.rs`)
- `RefreshWarning { message: String }` (`:29`). `RefreshResult` ALREADY has `warnings: Vec<RefreshWarning>` (`:25`) and its API-fail path populates it (`:158`). `FetchResult` (`:15`) has NO warnings field -> ADD `pub warnings: Vec<RefreshWarning>`; update its sole constructor (`:330`).
- `:189` cache-write fail: in `refresh_stale` loop. TODAY `eprintln!("warning: failed to write cache for {}: {}", id, e); continue;`. CHANGE: declare `let mut write_warnings = Vec::new();` before the loop, push `RefreshWarning { message: format!("failed to write cache for {}: {}", id, e) }` then `continue`. After the loop, the `warnings` built at `:201` (`refresh_schema_snapshot(...).into_iter().collect()`) -> prepend/extend with `write_warnings` before constructing `RefreshResult` at `:206`. NO eprintln.
- `:277` truncation: in `fetch_all`, right after `gh.issue_list(...)`. TODAY `eprintln!`. CHANGE: declare `let mut warnings: Vec<RefreshWarning> = Vec::new();` early in `fetch_all`; `if issues.len() as u64 == FETCH_LIMIT { warnings.push(RefreshWarning { message: format!("fetched exactly {} issues for type '{}'; there may be more", FETCH_LIMIT, type_def.name) }) }`. NO eprintln.
- `:327` schema-snapshot: in `fetch_all` AFTER `refresh_schema_snapshot` returns `Option<RefreshWarning>`. TODAY `if let Some(warning) = ... { eprintln!(...) }`. CHANGE: `warnings.extend(self.refresh_schema_snapshot(gh_graphql, repo));` (Option -> iter). Construct `FetchResult { fetched, new, removed, warnings }` at `:330`. NO eprintln.
- POST: `grep -n eprintln src/engine/issue_cache.rs` -> 0 hits. Engine I/O-pure per layering dictum.

### 2. CLI: print collected warnings to stderr (`src/cli/fetch.rs`, `src/cli/setup.rs`)
- `fetch.rs`: `result` from `fetch_all` (`:91`) currently feeds `summaries.push(TypeSummary{ fetched: result.fetched, ... })`. ADD after: for each `w in &result.warnings { eprintln!("warning: {}", w.message) }`. Human-mode only -- guard so `--json` path emits nothing extra to stdout (warnings go to stderr regardless, JSON on stdout unaffected). Behaviour preserved: schema/truncation/write-fail still reach the terminal in CLI.
- `setup.rs`: `result` from `fetch_all` (`:53`). ADD same stderr loop over `result.warnings` before the final `println!` summary.

### 3. TUI: carry warnings on the event, drop poll-thread `eprintln!` (`src/tui/infra/event_loop.rs`)
- `AppEvent::CacheRefresh` is payload-less (`app.rs` enum). CHANGE to `CacheRefresh { warnings: Vec<String> }`.
- Poll thread (`:523` spawn): TODAY discards each `FetchResult`, only `eprintln!` on `Err` (`:549`). CHANGE: declare `let mut warnings: Vec<String> = Vec::new();` before the `for type_def` loop. Per type: `match store.issue_cache.fetch_all(..) { Ok(r) => warnings.extend(r.warnings.iter().map(|w| w.message.clone())), Err(e) => warnings.push(format!("cache refresh failed for {}: {}", type_def.name, e)) }`. DROP the `:549` eprintln. Send `AppEvent::CacheRefresh { warnings }` (`:555`).

### 4. TUI: persistent gh-warning surface that survives re-validation (`src/tui/state/app.rs`)
- PROBLEM: `refresh_validation` (`:848`) OVERWRITES `validation_warnings` from `validate_full`, then `.extend(status_bar_warnings)`. Appending fetch warnings straight into `validation_warnings` would be clobbered on the next `refresh_validation` (which CacheRefresh itself calls).
- ADD field `pub gh_fetch_warnings: Vec<String>` (init `Vec::new()` at the struct literals: `:738`, `:3392`, `:3864`, and any test builders flagged by the compiler).
- In `refresh_validation` (`:848`): after the existing `.extend(status_bar_warnings)`, ADD `.extend(self.gh_fetch_warnings.iter().cloned())`. Now gh warnings ride every revalidation into the panel.

### 5. TUI: handler sets gh warnings before revalidating (`src/tui/infra/event_loop.rs:259`)
- `AppEvent::CacheRefresh { warnings }` arm: set `app.gh_fetch_warnings = warnings;` BEFORE `app.refresh_validation(config)` (refresh_validation reads the field). Replace cached docs as today. Empty `warnings` -> assign empty vec -> panel gains nothing (AC4).
- Update the other `CacheRefresh` send site if any (only `:555`). `GhPushResult` arm unchanged.

## Test Plan

- AC1 engine: unit on `fetch_all` with a fake forcing (a) cache-write error, (b) page-count == limit, (c) schema-snapshot refresh returning `Some(warning)` -> assert `result.warnings` contains all three messages; `grep eprintln issue_cache.rs == 0`.
- AC2 CLI: `fetch::run` with a fake producing a warning -> stderr contains `warning: ...`; `--json` stdout still parses and carries no warning key (warnings are stderr-only).
- AC3 TUI route: construct `App`, set `gh_fetch_warnings = vec!["w1"]`, call `refresh_validation` -> `validation_warnings` contains `"w1"`. Then simulate `CacheRefresh { warnings: vec!["w2"] }` handling (set field + refresh) -> panel shows `"w2"`; assert NO stderr write (no eprintln on the path).
- AC4 no-warn: `CacheRefresh { warnings: vec![] }` -> `gh_fetch_warnings` empty, `validation_warnings` == pure validation output (no spurious entries).
- Regression: existing `refresh_validation_*` tests still pass; `status_bar_warnings` extension order unchanged (validation, then status_bar, then gh_fetch).

## Notes

- Layering dictum: engine emits NO stderr -> warnings are data (`FetchResult.warnings`), callers decide sink. CLI -> stderr; TUI -> panel. This is the whole point of the slice.
- `validation_warnings` is the panel's data model; `refresh_validation` rebuilds it each call -> any warning that must persist across refresh needs a backing field it folds in (`status_bar_warnings` is the existing precedent; `gh_fetch_warnings` follows it).
- Set-then-refresh ordering in the CacheRefresh arm is load-bearing: refresh_validation reads `gh_fetch_warnings`, so assign first.
- `:277` truncation site is in a paging fn without a `FetchResult` -> thread `&mut Vec<RefreshWarning>` rather than restructure returns; smallest diff.
- Scope guard: do NOT change what the schema-snapshot warns (STORY-165) nor add dedup/severity/expiry (out of scope per story). Warnings accumulate as plain strings; clearing them is whatever already clears `validation_warnings` on reload.
- No new deps. `--json` on fetch/show/status untouched.
