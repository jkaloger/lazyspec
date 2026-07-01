---
title: Native macOS desktop app bundling the read-only web view
type: rfc
status: accepted
author: unknown
date: 2026-07-01
tags: []
related:
- related-to: RFC-052
---## Summary

Package the read-only web view (RFC-052, `lazyspec serve`) as a native macOS desktop application, so non-technical collaborators can review a lazyspec project without a terminal, a browser, or git. The app is a Tauri shell that opens a system webview (WKWebView) and drives every request through the **existing** axum `Router` in-process, with no TCP port bound. On launch the user picks a local lazyspec project folder (with a recents list); the folder is watched for changes and the view reloads live. Distribution is a `.app` bundle behind a new `app` cargo feature that depends on `web`. This RFC adds packaging and an in-process transport; it introduces no new rendering path.

## Motivation

RFC-052 gives the doc graph a URL, but reaching it still assumes the reviewer can run `lazyspec serve` from a checkout and open a browser at a localhost port. That assumption fails for the exact audience RFC-052 names: non-technical collaborators with no clone, no terminal, and no git fluency. A double-clickable app removes every one of those steps. The reviewer opens a folder they already have (synced via a shared drive, Dropbox, or handed to them) and reads the structured graph, `@ref` expansion, and relationship tree that justify lazyspec existing.

The narrow problem this RFC solves: **turn `serve` into an artifact a non-technical macOS user can open with no toolchain.** It does not change what is rendered, add editing, or introduce a network dependency.

## Goals

- A macOS `.app` bundle launches a Tauri window rendering a chosen lazyspec project, with no terminal, browser, or git required.
- On launch the app presents a native folder picker and remembers recently opened projects in app config.
- Every request from the webview is served by the existing axum `Router` in-process via a Tauri custom protocol; no TCP port is bound and no loopback socket is opened.
- The rendered surface is exactly RFC-052's: filterable/searchable list, per-document page with frontmatter + markdown + `@ref` expansion, and the topologically-sorted relationship tree.
- Editing continues to delegate to GitHub deep-links (RFC-052 `engine::github_url`); the app adds no write path.
- Changes under the opened folder trigger a `notify` watcher, an `Arc<Store>` swap, and a live htmx refresh. (The watcher must live in `web`/`engine`, not `tui` — see Design.)
- The Tauri/webview dependencies are confined to a new `app` cargo feature (depending on `web`); default and `web`-only builds gain nothing.

## Non-goals

- **Editing or write-back.** Inherited verbatim from RFC-052. The app is a viewer.
- **Authentication.** The webview is driven in-process by a single local user; there is no socket for a second party to reach, so the OAuth/membership gate (RFC-052 story 6, for hosted binds) is not used. The app never binds a hosted address.
- **Windows and Linux.** macOS only for this RFC. WebView2/WebKitGTK packaging is a later concern; the `app` feature is written to not preclude them, but nothing here targets them.
- **Code signing and notarization.** Deferred. The first artifact is an unsigned `.app` opened via right-click-Open. Signing is a follow-up once the shell is proven.
- **Runtime fetch of docs.** The app reads a local folder. It does not clone, pull, or call the GitHub API to obtain the store. (GitHub deep-links still point at github.com for editing, but that is a click-out, not a data source.)
- **Bundling a doc snapshot into the app.** The store is external (the chosen folder), not frozen into the binary. Rejected: a baked snapshot forces a rebuild+redistribute per edit and duplicates the data-distribution problem the shared folder already solves.
- **A second toolchain or a JS frontend.** Consistent with RFC-052: HTML is server-rendered by axum/askama; the shell is Rust (Tauri). No bundler, no SPA.

## Design

### Layering

The app is a thin shell over the existing `web` layer; it adds no rendering logic and preserves the inward dependency flow. `app -> web -> engine`, never `app -> tui` or `app -> cli`.

```
src/
  engine/          sync, UI-agnostic doc model / search / graph ordering
  cli/  tui/  web/ peer UI layers over engine
  app/             <- new; Tauri shell over web, macOS packaging
```

`app` is a new module gated behind a `app` cargo feature. `app` implies `web` (`app = ["web", "dep:tauri", ...]`), so building the app pulls the axum router and templates; building `web` alone does not pull Tauri.

### The in-process bridge (no port)

The core mechanism. RFC-052 exposes an axum `Router` bound to a TCP listener. The app reuses the **same** `Router` but never binds it. Instead it registers a Tauri custom URI scheme (e.g. `lazyspec://`); the webview navigates to `lazyspec://localhost/` and every subsequent request the WKWebView issues (page loads, htmx `GET`s, static assets) is handed to a protocol handler that drives the request through the axum `Router` as a `tower::Service`.

Concretely: the handler converts the webview's `http::Request` into the form `Router::call` accepts, awaits the response on the app's tokio runtime, and returns the `http::Response` to the webview. axum routes, askama templates, htmx fragments, and static assets are consumed verbatim; no route is reimplemented. The seam this requires already exists: `web::server::router(state) -> Router` was factored apart from the listener bind under RFC-052 and is already exercised in tests via `tower::ServiceExt::oneshot`. The app is the second consumer of that seam (hosted `serve` is the first), which retroactively justifies the extraction (principle 6).

Consequences:
- No open TCP port. Nothing on the machine or the network can reach the view except the app's own webview. This is why the RFC-052 auth gate is unnecessary here: there is no bind to protect.
- htmx works unchanged; it issues same-origin `lazyspec://` requests that the handler services.

### Project selection and app config

On launch, if no project is remembered (or the remembered path is gone), the app shows the native macOS folder picker (via Tauri's dialog API). The chosen path must contain a `.lazyspec/` project root; if not, the app reports it and re-prompts. Selected projects are appended to a recents list in the app's config directory (`~/Library/Application Support/lazyspec/` on macOS, resolved via a platform config-dir crate). A File menu offers Open Project and a recents submenu. Switching projects reloads the `Store` and re-points the watcher.

### Store loading and freshness

On opening a folder the app runs `Store::load` into an `Arc<Store>` shared with the router. A `notify` watcher observes `docs/` and `.lazyspec/` under the opened path; a change triggers a full reload and an atomic `Arc` swap, and the webview reflects edits on the next htmx poll/navigation. Reload-per-request and interval-reload are rejected for the same reasons RFC-052 gives.

**The watcher does not exist to reuse — it must be built here.** RFC-052's design cited "the `notify` watcher the TUI already uses", but that watcher lives in `src/tui/infra/event_loop.rs`, coupled to TUI state (`FileChange(notify::Event)` in `src/tui/state/app.rs`), and was never wired into `serve`. `web`/`app` cannot call into `tui` (principle 3). So the reload loop is authored fresh in `web` (or, if the watch-set + rebuild logic is genuinely shared with the TUI, lifted into `engine` — the same move RFC-052 made for `flatten_forest`). The `serve` HTTP path benefits from the same watcher; wiring it there is in scope for the story that adds it. This is not "reuse an existing seam"; it is new work.

### Runtime

Tauri runs an event loop on the main thread (required by macOS AppKit); axum/`tower` need a tokio runtime. The app owns a multi-thread tokio runtime for servicing protocol requests and running the watcher, distinct from Tauri's event loop. This is the only place the two async surfaces meet, and it is confined to the `app` feature.

### Packaging

`cargo tauri build` (or `tauri-bundler`) produces `lazyspec.app`. Unsigned for this RFC: users open it with right-click-Open once to clear Gatekeeper's first-run prompt. Signing/notarization is deferred to a follow-up. The bundle embeds the static web assets (CSS/JS/htmx) that `serve` ships, so no external asset path is needed at runtime.

## Interfaces

- `web::server::router(state: AppState) -> axum::Router` @accepted — **already exists** (RFC-052/STORY-176), factored out of the listener bind and driven in tests via `tower::ServiceExt::oneshot`. The app consumes it as-is; the bridge below is the only new transport.
- `web::watch(...)` (or `engine::watch(...)`) @draft — new `notify` reload loop over the opened project root, feeding the `Arc<Store>` swap. Does not exist today; the TUI watcher is `tui`-coupled and unreachable from `web`.
- `app::protocol` @draft — Tauri custom-scheme handler that adapts webview `http::Request` -> `Router` (`tower::Service`) -> `http::Response`. Internal module surface.
- `app::project` @draft — folder picker, `.lazyspec/` validation, recents persistence in the platform config dir.
- `app::run()` @draft — entry point under `#[cfg(feature = "app")]`: builds the tokio runtime, loads the store, starts the watcher, registers the protocol, opens the window.
- `app` cargo feature @draft — `app = ["web", "dep:tauri", "dep:tauri-build", ...]`. Present only when built with `--features app`.
- New bin/entry for the app (e.g. `src/bin/lazyspec-app.rs` or a `--features app` conditional in `main.rs`) @draft — decision recorded below.

## Decisions (ADRs to emit)

- **ADR: Tauri (wry/WKWebView) as the shell, not Electron or a raw wry window.** Rust-native, system webview, one toolchain — consistent with RFC-052's "no second toolchain" ADR. Tauri over bare wry/tao because it provides the dialog, menu, config-dir, and bundler plumbing this RFC needs without hand-rolling them.
- **ADR: in-process custom protocol driving the axum `Router`, no TCP bind.** Removes the open-port attack surface and the need for auth in the local single-user case; reuses every `serve` route verbatim. The second consumer of the `Router` justifies separating router construction from listener bind (principle 6).
- **ADR: read an external local folder, do not bundle a doc snapshot.** Keeps the app a viewer over live data the user controls; avoids per-edit rebuild/redistribute.
- **ADR: `app` cargo feature depending on `web`, macOS-only for now.** Isolates Tauri deps; default and `web` builds are untouched (mirrors RFC-052's `web`-feature isolation of the async stack).
- **ADR: defer code signing / notarization.** Unsigned `.app` first to prove the shell; signing is operational cost deferred behind the working renderer, as RFC-052 deferred hosting behind the working localhost view.

## Stories

1. **In-process protocol bridge.** `app` feature scaffold, a tokio runtime owned by the app (distinct from Tauri's AppKit event loop — the one place the two async surfaces meet), Tauri custom scheme, and the `http::Request` -> `web::server::router` (`tower::Service`) -> `http::Response` adapter. A hardcoded project path renders `GET /` in the webview. Proves the transport. (The router seam it consumes already exists from RFC-052; no seam extraction needed.)
2. **Project selection + recents.** Folder picker on launch, `.lazyspec/` validation, recents in the platform config dir, File menu (Open Project / recents), project switching (reload `Store`, re-point watcher).
3. **Watcher + live reload.** Author a `notify` reload loop reachable from `web`/`app` (new work — the TUI's watcher is `tui`-coupled and cannot be called; lift to `engine` only if the watch-set logic is genuinely shared). Wire it to the opened folder; `Arc<Store>` swap; htmx reflects edits; re-point on project switch. Wiring the same loop into hosted `serve` is in scope.
4. **macOS bundle.** `cargo tauri build` config, embedded static assets, unsigned `.app` output, first-run open instructions in the README.

Sequence: 1 precedes 2, 3, 4; 2 and 3 are independent given 1; 4 packages whatever exists. Signing is out of scope (future story).

## Risks and tradeoffs

- **Tauri build deps and the tokio↔AppKit boundary.** Tauri adds build tooling and forces a runtime split (AppKit event loop on the main thread, tokio for the router). Mitigation: confined to the `app` feature; `web`, `cli`, `tui`, and default builds are unaffected. The runtime boundary lives in one module (`app::run`).
- **Watcher is new, not reused.** The design's freshness story reads like "reuse the TUI watcher", but that watcher is `tui`-coupled and unreachable from `web` (principle 3); a fresh reload loop must be authored. Under-scoping story 3 as wiring rather than authoring is the trap. Mitigation: story 3 is explicit that the loop is new work, with the `flatten_forest` lift as the precedent for the `engine`-lift variant.
- **Stale shared folder.** A collaborator's synced copy can lag the repo. The app shows whatever is on disk; freshness is the sync tool's job, not the app's. Accepted: matches the "read a local path" data-source decision; the app is honest about rendering local state and deep-links to github.com for the authoritative version.
- **Unsigned distribution friction.** Right-click-Open is a rough first-run experience and some managed macOS fleets block unsigned apps outright. Mitigation: signing is a named follow-up; the unsigned build unblocks internal/technical-adjacent reviewers immediately.
- **Scope creep toward editing / multi-platform.** Same pressure RFC-052 documents. The non-goals (no write-back, macOS-only, no auth) are load-bearing and inherited deliberately.
- **Tension with the "simple doc tool" scope.** This ships a desktop application from a markdown CLI. Accepted on the same basis as RFC-052: it serves principle 1 (serve structured markdown) to the non-technical audience, reuses the `web` router wholesale, and adds zero surface to non-`app` builds.