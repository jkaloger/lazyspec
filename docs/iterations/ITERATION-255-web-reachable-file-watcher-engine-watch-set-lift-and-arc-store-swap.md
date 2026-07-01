---
title: 'Web-reachable file watcher: engine watch-set lift and Arc Store swap'
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-187
---## Objective

Author a fresh `notify` reload loop in `web` that watches an opened project's `docs/`+`.lazyspec/`, runs `Store::load`, and atomically swaps the router's shared `Arc<Store>`, keeping the last-good store on load failure. The pure watch-set logic is lifted into `engine` first.

## Satisfies

STORY-187 AC2 (watch → `Store::load` → atomic `Arc<Store>` swap), AC3 (consistent snapshot / no torn read), AC4 (`web`-layer reload loop compiles with no `crate::tui` / `AppEvent` / `App` dependency), AC6 (failed-reload resilience), AC7 (pure watch-set helper unit-tested independent of `notify`/UI). AC1, AC5, AC8 deferred — see Out of scope.

## Context

- Story + ACs: STORY-187 (`implements` RFC-054). Watcher is **new work, not TUI reuse.**
- Freshness / bridge design: RFC-054 §"Store loading and freshness" and §Design "The watcher does not exist to reuse — it must be built here."
- Engine-lift candidate: `watch_paths(root, config) -> Vec<PathBuf>`, the PRIVATE pure fn at `src/tui/infra/event_loop.rs:124` (no `notify`, no `App`). Lift precedent: `flatten_forest` in `src/engine/graph.rs:196`, consumed by `web` at `src/web/routes.rs:17`.
- MUST NOT depend on: `rewatch` (`event_loop.rs:155`), `reload_session`, `AppEvent::FileChange` (`src/tui/state/app.rs`) — all TUI-coupled (principle 3: `app -> web -> engine`, never `web -> tui`).
- Shared state to swap: `AppState.store: Arc<Store>` at `src/web/server.rs:24` (held by clone today — the swap needs an interior-mutable holder; router built at `src/web/server.rs:42`). `serve` runtime construction at `src/web/server.rs:57`.
- Touch: `src/engine/` (relocated `watch_paths` + its tests), `src/tui/infra/event_loop.rs` (call the engine fn instead of the local one), `src/web/watch.rs` (new), `src/web/server.rs` (swappable store holder), `src/web/mod.rs` (module decl).

## Principles / conventions

- Layering: `app -> web -> engine`; no `web -> tui` / `app -> tui` (principle 3). No indirection before a second consumer (principle 6) — the swappable-store holder is introduced only as this loop needs it.
- Rust ecosystem norms for the async/`notify` surface and feature gating (principle 5).

## Tasks

1. Lift `watch_paths` from `src/tui/infra/event_loop.rs:124` into `engine` (follow the `flatten_forest` lift): make it `pub`, move its unit coverage with it, and update the TUI's `rewatch` to call the engine fn. Behaviour unchanged (AC7).
2. Give the router a swappable store: replace `AppState.store: Arc<Store>` reads with an interior-mutable holder (e.g. `arc-swap` or `Arc<ArcSwap<Store>>`) so handlers load a consistent snapshot per request and a swap is visible only to subsequent requests — no lock held across a request (AC3). Keep `router`/handler signatures otherwise intact.
3. Test-first: add `src/web/watch.rs` tests asserting the loop (a) rebuilds+swaps the shared store on a relevant change under `docs/`/`.lazyspec/`, (b) keeps the prior store on a `Store::load` `Err`, and (c) references no `crate::tui`, `AppEvent`, or `App` (AC4, AC6).
4. Implement `web::watch` (e.g. `watch(root, handle) -> ...`): a `notify` watcher over `engine::watch_paths(root, config)`; on a relevant event run `Store::load(root)`, and on `Ok` atomically swap the shared holder, on `Err` skip the swap and keep serving the last-good store (AC2, AC6). Own its runtime/thread within `web`; do not touch Tauri or `serve` wiring.

## Out of scope

- Wiring the loop into hosted `serve` (AC8) → ITERATION-256.
- Project-switch re-pointing / stop-old-watch-new (AC5) → ITERATION-256.
- App/webview integration, the freshness-on-next-poll end-to-end path (AC1) → verified once wired (ITERATION-256 / STORY-186 selection).
- Debounce/coalescing tuning and `reload_session`'s richer config-hot-reload semantics (RFC-054 non-goals). Any TUI refactor beyond the `watch_paths` call site; the TUI watcher is left as-is.
- Push/WebSocket/SSE transport; editing/write-back.

## Verification

- `engine::watch_paths` unit test returns the same set as the moved TUI coverage (AC7).
- Loop test: touch a file under `docs/` → shared store observes the reload; make `Store::load` fail → served store is unchanged and the loop survives (AC2, AC6).
- `grep` the new `web::watch` module for `tui`/`AppEvent`/`App` → none (AC4).