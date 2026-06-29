---
title: 'serve skeleton: feature flag, axum server, Arc<Store>, document list'
type: story
status: accepted
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
- blocks: STORY-177
- blocks: STORY-178
- blocks: STORY-179
- blocks: STORY-180
- blocks: STORY-181
---## Context

RFC-052 adds a read-only web view for lazyspec documents behind a `web` cargo feature. This story is the foundational slice: it stands up the `lazyspec serve` subcommand, the feature-gated async stack (`tokio` + `axum`), the shared `Arc<Store>`, and the first route -- `GET /` rendering a filterable document list. It deliberately ships without authentication and binds to loopback only; auth and hosted bind arrive in STORY-181. Every other web story (177-180) depends on this skeleton existing, so it precedes all of them.

The constraints are load-bearing: the async stack must be absent from default builds (no `tokio`/`axum` without `--features web`), and the `web` module depends only on `engine`, never on `cli` or `tui` (convention principle 3).

## Acceptance Criteria

- **Given** a default build (`cargo build` with no `--features web`)
  **When** the binary is compiled
  **Then** no async runtime or HTTP dependency (`tokio`, `axum`) is linked, and no `serve` subcommand is present.

- **Given** a build with `--features web`
  **When** the user runs `lazyspec serve`
  **Then** an HTTP server starts bound to `127.0.0.1` on the default port, loads the project `Store` once into an `Arc<Store>`, and logs the bound address.

- **Given** a running `serve` instance
  **When** a client requests `GET /`
  **Then** the response is server-rendered HTML listing all documents grouped by type, each row showing id, title, and status.

- **Given** the document list page
  **When** the user applies a status or tag filter
  **Then** htmx updates the list in place to the matching subset without a full page reload.

- **Given** a `serve` instance and a `--port <n>` flag
  **When** the server starts
  **Then** it binds the specified port; absent the flag it binds the default port `8787`.

- **Given** the `web` module source
  **When** the dependency graph is inspected
  **Then** `web` imports only from `engine`, never from `cli` or `tui`.

## Scope

### In Scope

- `web` cargo feature gating the entire `src/web/` module and its async dependencies.
- `lazyspec serve [--port <n>]` subcommand, present only under `--features web`.
- `axum` + `tokio` HTTP server scaffolding (`web::server`, `web::routes`).
- One-time `Store::load` into `Arc<Store>` shared across handlers.
- `GET /` document list grouped by type, with htmx-driven status/tag filters.
- `askama` template scaffolding for the list view.
- Loopback (`127.0.0.1`) bind only; no auth.

### Out of Scope

- Document page, `@ref` expansion, markdown-to-HTML (STORY-177).
- Search route (STORY-178).
- Graph render and the `flatten_forest`/`compare_siblings` lift (STORY-179).
- GitHub deep-links (STORY-180).
- OAuth, membership check, sessions, hosted (non-loopback) bind (STORY-181).
- The `--bind <addr>` flag: deferred to STORY-181, where bind policy is tied to auth. This skeleton hardcodes loopback and exposes only `--port`.
- File-watcher reload on `docs/` changes (lands with the first route that benefits; not required for the skeleton).
