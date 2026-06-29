---
title: 'web serve skeleton: feature gate, axum server, Arc<Store>, GET / document list'
type: iteration
status: accepted
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-176
---

## Objective

Stand up the feature-gated `web` module: a `lazyspec serve [--port <n>]` subcommand (present only under `--features web`) that loads the `Store` once into an `Arc<Store>` and serves `GET /` as an htmx-filterable, server-rendered document list bound to loopback.

## Satisfies

STORY-176 — all six ACs (feature isolation, loopback serve, `GET /` list, htmx filter, `--port` default 8787, web→engine-only layering). These ACs are one tightly-coupled skeleton, not separable slices.

## Context

- Story + ACs: STORY-176 (in/out of scope is load-bearing).
- Design (stack, layering, serving-the-store rationale): RFC-052 §Design — "Layering", "Stack", "Serving the store", and the `GET /` route line under "Routes".
- Conventions: docs/convention/CONVENTION.md — principle 3 (engine → cli/tui/web; layers never depend on each other) governs this work.
- Existing types to consume (do not modify): `engine::store::Store` (`Store::load(root: &Path, config: &Config) -> Result<Self>`, Send+Sync), `Store::all_docs() -> Vec<&DocMeta>` / `Store::list(&Filter)`, `engine::store::DocMeta` (`id`, `title`, `doc_type`, `status`, `tags`), `engine::store::Filter`.
- Touch: `Cargo.toml` (add `[features] web`, gate `tokio`/`axum`/`askama` as optional deps under it), `src/lib.rs` (`#[cfg(feature = "web")] pub mod web;`), `src/web.rs` + `src/web/{server,routes,render}.rs` (new), `src/cli.rs` (add feature-gated `Serve { port: Option<u16> }` variant), `src/main.rs` (feature-gated dispatch arm).

## Tasks

1. Add a `web` feature to `Cargo.toml` gating `tokio` (rt-multi-thread, macros), `axum`, and `askama` as optional dependencies. Confirm `cargo build` (no features) pulls in none of them.
2. Create `src/web/` with `server` (build router, bind `127.0.0.1:<port>`, default 8787, hold `Arc<Store>` as axum state, log bound address), `routes` (`GET /` handler + the htmx filter-fragment handler), and `render` (askama templates for the full list page and the filterable list fragment). Group rows by `doc_type`; each row shows id, title, status. Import only from `engine`.
3. Add the `#[cfg(feature = "web")]` `Serve { port: Option<u16> }` clap variant in `src/cli.rs` and the matching dispatch arm in `src/main.rs` that loads the `Store` into an `Arc` and calls `web::server`.
4. Wire htmx status/tag filtering: filter controls on the list page issue requests that the fragment handler answers with the matching subset, swapped in place (reuse `Store::list(&Filter)` for the subset).
5. Cover with tests under `tests/integration/` per repo precedent: `GET /` returns 200 HTML containing seeded doc ids grouped by type; a filtered request returns only the matching subset; default port is 8787 and `--port` overrides it. Gate the test module on `#[cfg(feature = "web")]`.

## Out of scope

- Document page, `@ref` expansion, markdown-to-HTML → STORY-177.
- Search route → STORY-178.
- Graph render + the `flatten_forest`/`compare_siblings` engine lift → STORY-179.
- GitHub deep-links → STORY-180.
- OAuth, membership check, sessions, `--bind`/hosted (non-loopback) bind → STORY-181. This skeleton hardcodes loopback and exposes only `--port`.
- `notify` file-watcher reload of `Arc<Store>` — deferred; the skeleton loads once at startup.

## Principles / conventions

- docs/convention/CONVENTION.md principle 3: `web` imports only from `engine`, never `cli` or `tui`.
- Default builds stay async-free: `tokio`/`axum` are reachable only through `--features web`.

## Verification

- `cargo build` with no features links no `tokio`/`axum` and exposes no `serve` subcommand; `cargo build --features web` exposes it.
- `cargo run --features web -- serve` logs a `127.0.0.1:8787` bind; `--port 9000` binds 9000.
- Inspect `src/web/` imports: no `use crate::cli` / `use crate::tui`.

