---
title: TUI search blocks UI thread per keystroke
type: bug
status: fixed
author: unknown
date: 2026-07-21
tags: []
related:
- related-to: STORY-130
---## Context

TUI fuzzy search (STORY-130) has a noticeable per-keystroke delay. CLI search is fine (one query per invocation). fzf-style tools feel instant on far larger corpora.

## Root Cause

Per-keystroke synchronous nucleo scoring over every doc's full body, on the UI thread. Debug builds multiply the cost ~6-10x.

Measured (718 docs, 3.6MB body text):

| Measurement | Result |
|---|---|
| Debug build, per query | 275ms–960ms ("f" → "fuzzy search ranking") |
| Release build, per query | 38ms–98ms |
| Body scoring share | ~100ms of ~98ms total (release) — bodies are the cost |
| Body cache clone | 187µs — negligible |
| Warm vs cold | identical — body_cache works, irrelevant to the delay |

Causal chain:

1. `App::update_search` (`src/tui/state/app.rs:2200`) runs `Store::search` synchronously per keystroke on the UI thread. No debounce, no worker.
2. `Store::search` (`src/engine/store.rs:364`) runs `pattern.indices` over every full body — 3.6MB per keystroke. Multi-word queries score one pass per atom.
3. The TUI is run via `cargo run` (debug build), so nucleo is unoptimized: 0.3–1s per keystroke. Typed chars queue behind the blocked event loop.
4. Minor: `draw_search_overlay` (`src/tui/views/overlays.rs:993`) re-parses the Pattern and constructs a new Matcher per row per frame, and builds per-char spans for all results rather than the visible window.

ADR-013 accepted a "synchronous first-query body read" tradeoff, but the real cost is per-keystroke scoring, not first-read I/O (warm == cold above).

## Expected vs Actual

- **Expected:** typing in the search overlay updates results with no perceptible lag; UI stays responsive while results compute.
- **Actual:** each keystroke blocks the event loop 0.3–1s (debug) / 40–100ms (release); input feels laggy and chars queue up.

## Repro

1. `cargo run` in this repo, open TUI, press `/`.
2. Type a multi-word query, e.g. `fuzzy search ranking`.
3. Observe per-keystroke lag; input echo stalls behind search.

Benchmark repro: load Store, loop `store.search(q, &fs)` for growing queries, time each call (debug vs `--release`).

## Task Breakdown

### Task 1: Optimize nucleo in dev builds

Add to `Cargo.toml`:

```toml
[profile.dev.package.nucleo]
opt-level = 3
```

Rationale: search cost is dominated by nucleo's inner matching loop; `cargo run` (the documented way to run the dev TUI) currently pays the unoptimized cost (~6-10x). This override compiles only the nucleo dependency with optimizations in dev builds. Verify `cargo build` succeeds.

### Task 2: Async search worker with spinner

Move per-keystroke search off the UI thread; show a spinner while results are pending; drop stale results.

- **Engine:** add an owned, thread-safe search corpus API so a worker thread can search without borrowing `Store`. Matching stays in the engine (RFC-043 principle 3) — the TUI never owns the scoring algorithm. Suggested shape: `Store::search_corpus(&self, fs) -> SearchCorpus` (owned snapshot of per-doc searchable fields: path, title, tags, path string, body — bodies via the existing body cache; snapshot clone measured at ~200µs for 3.6MB) and `SearchCorpus::search(&self, query) -> Vec<...>` reusing the existing scoring logic (same Pattern config, score floor, field selection, sort). `Store::search` keeps its signature (CLI/web unchanged) — either delegating to the corpus path or sharing the per-doc scoring internals; avoid duplicating the scoring loop in two places.
- **TUI state (`src/tui/state/app.rs`):** `update_search` no longer calls `Store::search` synchronously. Instead it bumps a `search_generation: u64`, marks search pending, and hands the (query, generation) to a background worker. New `AppEvent::SearchResults { generation, results }` (follow the existing `CreateProgress`/`CreateComplete` pattern); the handler applies results only when `generation == self.search_generation`, else drops them. Empty query short-circuits: clear results, not pending, no dispatch.
- **Worker (`src/tui/infra/event_loop.rs`):** background thread receiving (query, generation) over a crossbeam channel (single long-lived worker draining to the latest query, per DICTUM-007: no threads mutating state directly; results come back as `AppEvent` through the existing `tx`). Worker owns/refreshes the `SearchCorpus`; rebuild the corpus on the same triggers that clear `body_cache`/`expanded_body_cache` (file change, cache refresh) — a stale-corpus flag re-snapshotting on next query is fine.
- **Spinner:** while pending, render the existing spinner (`crate::spinners`, `frame_style` in `tui/views/colors.rs`) in the search overlay results block title or body, matching how the create form shows progress. Spinner clears when the matching-generation results land.
- **Tests (per DICTUM-004):** state-level, no terminal: (a) `update_search` marks pending and bumps generation; (b) applying `SearchResults` with a stale generation leaves results untouched; (c) matching generation applies results and clears pending; (d) empty query clears results without pending. Engine: `SearchCorpus::search` returns identical results to `Store::search` for the same fixture set.

### Task 3: Bound search overlay render cost

In `draw_search_overlay` (`src/tui/views/overlays.rs`):

- Build `ListItem`s only for the visible window (derive from the results area height and `search_selected`), not for all results. Keep selection/scroll behaviour correct at the window edges.
- Hoist per-row work: parse the query Pattern and construct the Matcher once per frame, not once per row. If that needs an engine-side reusable helper (e.g. a struct holding parsed Pattern + Matcher exposing `indices(&mut self, text)`), add it beside `match_indices` and reimplement `match_indices` on top of it so there is one matcher-config site.
- Tests: engine helper parity with `match_indices`; view-layer logic that computes the visible window slice is a pure function — unit test the windowing math (selected near top, middle, bottom).

## Acceptance Criteria

- [x] Typing in the TUI search overlay never blocks the event loop on scoring; keystrokes echo immediately (debug build, this repo's corpus).
- [x] Spinner visible while a search is computing; disappears when results land.
- [x] Stale results (from a superseded query) are never displayed.
- [x] `SearchCorpus::search` ranking identical to `Store::search` (same fixtures, same order).
- [x] CLI `search` and web view behaviour unchanged.
- [x] `cargo build` compiles nucleo with opt-level 3 in dev profile.
- [x] Full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.

## Fix Direction

1. **Async search worker + spinner.** Run `Store::search` off the UI thread via the existing `AppEvent` pattern. Generation counter drops stale results. Show a spinner (spinners module) in the results pane while a search is pending. Keystrokes stay responsive at any corpus size.
2. **`[profile.dev.package.nucleo] opt-level = 3`** in `Cargo.toml`. One line; makes `cargo run` search roughly release speed.
3. **Render only the visible window** in `draw_search_overlay`, and hoist Pattern/Matcher construction out of the per-row loop.

Not pursued: nucleo's high-level streaming crate (fzf's model). Right shape long-term, but corpus is 3.6MB; worker + spinner suffices (principle 6).

Web view and CLI unaffected: both run one search per request; no per-keystroke path.

