---
title: 'App bridge walking skeleton: render GET / in a Tauri window via the web router'
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-185
---

## Objective

A Tauri window renders `GET /` from a hardcoded project by driving the webview through the existing `web::server::router` in-process, with no TCP port bound.

## Satisfies

STORY-185 AC1–AC8.

## Context

- Story + ACs: STORY-185 (`implements` RFC-054).
- Bridge design, runtime split, custom-scheme rationale: RFC-054 §Design "The in-process bridge".
- Router seam to consume as-is: `src/web/server.rs:42` — `web::server::router(state: AppState) -> axum::Router`, a `tower::Service` already driven via `tower::ServiceExt::oneshot` in that file's tests.
- Static routes to service through the scheme: `/static/lazyspec.css`, `/static/fonts/{name}` (`src/web/assets.rs`).
- `AppState`/`Store` construction to mirror: how `serve` builds state in `src/web/server.rs` and its call site in `src/main.rs`.
- Feature wiring: `Cargo.toml` (`[features]`, `notify`/`tokio`/`axum` already present).

## Principles / conventions

- Convention doc (layering): `app -> web -> engine`, never `app -> tui`/`cli` (principle 3).
- Rust ecosystem norms for the async surface + feature gating (principle 5); no indirection before a second consumer (principle 6).

## Tasks

1. Add an `app` cargo feature to `Cargo.toml`: `app = ["web", "dep:tauri", ...]`; add `tauri` (+ any `tauri-build`) as optional deps. Confirm default/`web`/`cli`/`tui` builds pull no Tauri (AC1, AC2).
2. Create `src/app/mod.rs` (module gated `#[cfg(feature = "app")]`), exposing `app::run()` as the entry point. Record the bin-vs-conditional-`main.rs` choice inline where the RFC left it open.
3. In `app::run()`, build an app-owned multi-thread tokio runtime distinct from Tauri's AppKit main-thread event loop; keep the runtime boundary inside the module (AC7).
4. Load a hardcoded project path via `Store::load` into `Arc<Store>` and construct `AppState` the same way `serve` does (AC8).
5. Register a Tauri custom URI scheme (e.g. `lazyspec://`); point the webview at `lazyspec://localhost/` on launch.
6. Implement the protocol handler: adapt the webview `http::Request` → `web::server::router` (`tower::Service`, `.call`/`oneshot`) → `http::Response`, awaited on the app runtime; route `/static/*` through the same path (AC5, AC6). Reimplement no route.
7. Open the window; verify it renders the RFC-052 list identical to `serve`'s `GET /` (AC3), and that no TCP port / loopback socket is bound while running (AC4).

## Out of scope

- Folder picker, `.lazyspec/` validation, recents, File menu — STORY-186 (path is hardcoded here).
- `notify` watcher, `Arc<Store>` swap, live reload — STORY-187.
- `cargo tauri build`, embedded-asset bundle, unsigned `.app` — STORY-188.
- Code signing / notarization; Windows/Linux; auth (no socket bound); any edit/write path.
- Any change to `web::server::router`, routes, or templates — consumed as-is.

## Verification

- Launch with `--features app` → window shows the document list; `lsof`/`netstat` shows no bound TCP port for the process while the window is open (AC3, AC4).
- `cargo build` (no `--features app`) and `cargo build --features web` succeed with no Tauri in the dependency tree (AC1, AC2).
