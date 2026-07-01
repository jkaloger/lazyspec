---
title: Wire live-reload watcher into serve and re-point on project switch
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-187
---## Objective

Wire the `web::watch` reload loop (ITERATION-255) into hosted `lazyspec serve` and re-point it at the new root when the open project switches, so both the HTTP path and the app gain live reload driven by one loop.

## Satisfies

STORY-187 AC7 (hosted `serve` reflects on-disk edits), AC5 (switch stops the old watch and re-points at the new root).

Already covered by ITERATION-255: AC1, AC2, AC3, AC4, AC6, AC8 (the `web::watch` loop, `Store::load` + atomic `Arc` swap, in-flight consistency, no-`tui` boundary, failed-reload resilience, watch-set helper). This slice only *wires and re-points* that loop; it re-authors none of it.

## Context

- Story + ACs: STORY-187 (`implements` RFC-054, story 3).
- Preceding slice this builds on: ITERATION-255 — authored `web::watch` (the `notify` loop), the `engine` watch-set lift, and the `Arc<Store>` swap seam. Consume its swap handle and loop entry as-is; do not restate its internals.
- Switch seam to integrate with: STORY-186 "Switch re-points the watcher" AC + ITERATION-254 — the switch call site rebuilds `AppState` and exposes the re-point hook this slice fills.
- `serve` entry + the runtime it owns: `src/web/server.rs:57` (`serve`) / `:65` (`serve_async`); router + shared `AppState` at `:42` / `:22`.
- `serve` call-site state construction to preserve: `src/main.rs:576`–`598`.

## Principles / conventions

- Layering (RFC-054): `app -> web -> engine`; never `web -> tui`. The loop stays in `web`; `serve` and the `app` switch seam are its two callers.
- No indirection before a second consumer: `serve` is that second consumer — the loop already runs under `app`, so wiring it here validates the shared seam rather than duplicating it.

## Tasks

1. In `serve`/`serve_async` (`src/web/server.rs`), spawn the ITERATION-255 `web::watch` loop against the served project root on the same tokio runtime as `axum::serve`, holding the swap handle over the router's shared store. Do not block or race server startup.
2. At the STORY-186/ITERATION-254 switch call site, replace the re-point stub with a real call: stop the previous watch and start `web::watch` on the new root, targeting the rebuilt `AppState`'s store handle (AC5).
3. Expose whatever minimal watch-handle/stop surface tasks 1–2 need from `web::watch`; keep it the single seam both `serve` and the switch use.
4. Test (`web` layer): edit a doc under a served root, assert the next request through `router` reflects it (AC7); then switch roots and assert edits to the new root drive reloads while edits to the old root do not (AC5).

## Out of scope

- The `web::watch` loop body, watch-set helper, `Arc` swap mechanics, and resilience — all ITERATION-255 (AC1–AC4, AC6, AC8); consumed, not modified.
- The folder picker, recents, validation, and `AppState` rebuild — STORY-186 / ITERATION-254; this slice only calls the re-point hook they expose.
- Push/SSE/WebSocket freshness, debounce tuning, config-hot-reload semantics (STORY-187 non-goals).
- Any `web -> tui` dependency or TUI watcher refactor; any edit/write-back path.

## Verification

- Switch boundary: after switching from project A to B, a save under A's `docs/` triggers no reload; the equivalent save under B's `docs/` is reflected on the next request (AC5).
- Hosted path: with `lazyspec serve` running, editing a served `.md` is reflected on the next HTTP request with no restart (AC7).