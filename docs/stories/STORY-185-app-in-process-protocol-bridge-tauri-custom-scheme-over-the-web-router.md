---
title: 'App in-process protocol bridge: Tauri custom scheme over the web router'
type: story
status: accepted
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-054
---## Context

RFC-052 gave the doc graph a URL served by an axum `Router`; RFC-054 packages that
view as a native macOS app so a non-technical reviewer can open a lazyspec project
without a terminal, a browser, or a bound TCP port. The whole app rests on one
unproven mechanism: can a Tauri webview be driven entirely by the **existing** axum
router, in-process, with no socket? Every later story (project picker, live-reload
watcher, `.app` bundle) is pointless until that transport works.

This is the walking skeleton for the desktop app. As a lazyspec maintainer, I want a
Tauri window that renders `GET /` from a hardcoded project by routing the webview's
requests through `web::server::router` in-process, so the "no port, reuse every route
verbatim" premise of RFC-054 is proven before any product surface is layered on top.

The seam this depends on already exists: `web::server::router(state: AppState) ->
axum::Router` (`src/web/server.rs:42`) was factored apart from the listener bind under
RFC-052 and is a `tower::Service` already exercised via `tower::ServiceExt::oneshot`.
This story is its second consumer. No route is reimplemented; no rendering logic is
added. The value observed is deliberately thin — one page in a window — because the
value being proven is the *transport*, not the product.

Layering (RFC-054 principle 3): `app -> web -> engine`. The new `app` module and
feature must never reach into `tui` or `cli`.

## Acceptance Criteria

- **Given** a build with default features (no `--features app`),
  **When** the project is compiled,
  **Then** no Tauri, wry, or webview dependency is pulled in, and the `web`, `cli`,
  `tui`, and default builds are unchanged.

- **Given** the new `app` cargo feature,
  **When** its dependency declaration is inspected,
  **Then** `app` implies `web` (e.g. `app = ["web", "dep:tauri", ...]`) so that
  building the app pulls the axum router and templates, while building `web` alone
  does not pull Tauri.

- **Given** the `app` feature is enabled and a valid lazyspec project path is
  hardcoded in the entry point,
  **When** the app is launched (`app::run()`),
  **Then** a Tauri window opens showing the RFC-052 document list rendered by the
  existing `router`, identical to what `lazyspec serve` renders for `GET /`.

- **Given** the running app,
  **When** the operating system's open TCP ports and loopback sockets are inspected
  while the window is displayed,
  **Then** the app has bound no TCP port and opened no loopback socket; the view is
  reachable only from the app's own webview.

- **Given** the webview navigating to the app's custom URI scheme (e.g.
  `lazyspec://localhost/`),
  **When** the webview issues a request (page load or static asset such as the
  stylesheet or a font),
  **Then** a protocol handler converts the webview's `http::Request` into the form
  `Router::call` accepts, awaits the response on the app-owned tokio runtime, and
  returns the resulting `http::Response` to the webview — with no route
  reimplemented.

- **Given** the app registers a custom scheme handler and the CSS/font routes served
  under `GET /static/...`,
  **When** the list page loads in the webview,
  **Then** its linked static assets resolve through the same in-process handler and
  the page is styled, confirming same-origin `lazyspec://` requests are serviced end
  to end.

- **Given** macOS requires the AppKit event loop on the main thread and axum/`tower`
  require a tokio runtime,
  **When** the app starts,
  **Then** the app owns a multi-thread tokio runtime for servicing protocol requests
  that is distinct from Tauri's AppKit event loop, and this runtime boundary is
  confined to the `app` module (e.g. `app::run`).

- **Given** the hardcoded project path,
  **When** `Store::load` runs at startup,
  **Then** the loaded store is placed in an `Arc<Store>` inside the `AppState` shared
  with the router, matching how `serve` constructs its state.

## Scope

### In Scope

- A new `src/app/` module gated behind a new `app` cargo feature that depends on
  `web`.
- An app-owned multi-thread tokio runtime, created distinct from Tauri's AppKit main
  event loop, confined to the app module.
- Registration of a Tauri custom URI scheme (e.g. `lazyspec://`) that the webview
  navigates to on launch.
- A protocol handler adapting webview `http::Request` -> `web::server::router`
  (`tower::Service`) -> `http::Response`, servicing page loads and the existing
  `/static/...` asset routes.
- Loading a **hardcoded** project path into an `Arc<Store>`/`AppState` and rendering
  `GET /` in the window.
- An entry point under `#[cfg(feature = "app")]` (e.g. `app::run()`), with the
  bin-vs-conditional-`main.rs` decision recorded per the RFC's open interface item.

### Out of Scope

- The native folder picker, `.lazyspec/` validation, recents list, and File menu
  (RFC-054 story 2 — the path is hardcoded here).
- The `notify` file watcher, `Arc<Store>` swap, and live htmx reload (RFC-054 story
  3).
- `cargo tauri build`, embedded static assets in a distributable bundle, and the
  unsigned `.app` output (RFC-054 story 4).
- Code signing and notarization (deferred RFC-wide).
- Any editing/write path — editing remains delegated to GitHub deep-links (RFC-052 /
  RFC-054 read-only-view invariant).
- Authentication — no socket is bound, so the RFC-052 OAuth/membership gate is not
  used here.
- Windows and Linux packaging.
- Any change to `web::server::router` or the routes/templates it serves; this story
  consumes the seam as-is and reimplements no route.