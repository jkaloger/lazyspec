---
title: "Read-only web view for lazyspec docs"
type: rfc
status: accepted
author: "unknown"
date: 2026-06-29
tags: []
related: []
---

## Summary

Add a read-only, hosted web view for lazyspec documents, served by a new `lazyspec serve` subcommand behind a `web` cargo feature. It renders the document graph (list, search, rendered markdown, relationship tree) in a browser gated by GitHub OAuth, and links each document out to its GitHub representation for editing. The goal is to give non-technical collaborators a way to _review_ specs without a terminal, a clone, or git fluency, while editing remains delegated to GitHub's own UI.

## Motivation

The narrow problem this RFC solves: **give that audience read access to the structured doc graph through a URL they already have credentials for.** Editing is a larger problem (write-back, validation, concurrency, auth-for-write) deliberately split into a separate RFC. Without a viewer, the only options are "teach git" (high friction for the audience) or "let them read raw markdown on github.com" (loses the structure, relationships, and `@ref` expansion that justify lazyspec existing). A read-only view is the smallest thing that serves the audience and reuses the engine as-is.

## Goals

- A `lazyspec serve` command starts an HTTP server that renders the documents in the current lazyspec project.
- A non-technical user reaches the view at a URL, authenticates with their existing GitHub account, and is allowed in iff they are a collaborator/member on the repo.
- The view renders: a filterable/searchable document list; a per-document page with structured frontmatter header, markdown body rendered to HTML, and `@ref` directives expanded inline; a relationship graph as a topologically-sorted tree mirroring the TUI's graph.
- Every document carries an outbound link to its GitHub representation (blob, issue, or milestone) so editing happens on github.com.
- The web layer depends only on `engine`, never on `tui` or `cli` (principle 3).
- Default builds (without `--features web`) gain no async runtime and no HTTP dependencies.

## Non-goals

- Editing or write-back of any kind. No create/update/delete from the browser.
- OAuth scopes beyond read identity + membership check. No write tokens.
- Concurrency or conflict handling (a read-only view holds no locks and races nothing).
- Comments, presence, realtime updates, websockets.
- A PR-based or approval review flow.
- A JavaScript SPA, a second toolchain, or a separate frontend build. HTML is server-rendered; JS is limited to htmx and one markdown/diagram widget loaded as a static asset.
- Multi-repo or multi-project hosting. One server instance serves one project.

## Design

### Layering

A new `src/web/` module joins `cli/` and `tui/` as a peer layer over `engine/`. Dependencies flow inward: `web -> engine`, never `web -> tui` or `web -> cli`. The layer is gated behind a `web` cargo feature so the async stack (tokio, axum) is absent from default builds.

```
src/
  engine/    sync, UI-agnostic; the single authority for doc model, search, graph ordering
  cli/  tui/  web/   <- new peer layer
```

### Stack

- `axum` + `tokio` for the HTTP server (the only async surface in the codebase; isolated to this feature).
- `askama` for compile-time-checked HTML templates.
- `htmx` for interactivity (search-as-you-type, filter) without a client framework.
- `pulldown-cmark` (already a dependency) for markdown -> HTML.
- A single static JS asset for the markdown body / mermaid diagrams; no bundler.

### Serving the store

`lazyspec serve` loads the `Store` once into an `Arc<Store>` shared across request handlers. Because the view is read-only, no interior mutability or locking is required for correctness. Freshness comes from the `notify` file watcher the TUI already uses: a change under `docs/` or `.lazyspec/` triggers a full `Store::load` and an atomic swap of the `Arc`. Reload-per-request is rejected (it re-walks the whole store on every hit); reload-on-interval is rejected (it adds staleness with no benefit over the watcher). The watcher is the one committed strategy.

### Routes

- `GET /` -> document list, grouped by type, with status/tag filters (htmx-driven).
- `GET /search?q=` -> reuses the engine search used by `lazyspec search`.
- `GET /doc/{id}` -> document page: frontmatter header, body rendered to HTML, `@ref` directives expanded (reusing the engine's existing expand-references logic that backs `show --expand-references`).
- `GET /graph` -> relationship graph as a topologically-sorted tree.
- `GET /auth/github`, `GET /auth/callback` -> OAuth handshake.

### Authentication

GitHub OAuth (web application flow). On callback, the server checks the authenticated user's membership/collaborator status on the configured repo and admits iff true; permissions mirror the repo. Sessions are signed cookies. The OAuth client id/secret and the repo coordinates are configured out-of-band (env/config), not committed. Read scope only.

The membership check is a runtime dependency on the GitHub API: it runs once at session establishment, and the result is cached in the session for a bounded TTL rather than re-checked per request (per-request checks would couple every page load to GitHub latency and rate limits). When the GitHub API is unavailable at session establishment, the server fails closed (denies the session) rather than admitting unverified users. Revocation is therefore eventually consistent, bounded by the session TTL.

**Auth and bind are not independent.** A hosted bind (any non-loopback address) requires OAuth to be configured; the server refuses to start hosted without it. The no-auth path exists only for loopback (`127.0.0.1`) single-user local development. Hosted-without-auth is not a supported configuration.

### Graph rendering

The TUI's topologically-sorted tree is built in two stages today: `resolve_forest` / `topo_order` in `engine/context.rs` (Kahn's algorithm, reusable), and `flatten_forest` / `compare_siblings` in `tui/state/graph.rs` (DFS flatten + sibling sort, currently trapped in the TUI layer). To render the same ordering in HTML without `web -> tui`, the flatten + sibling-sort logic is lifted from `tui/` into `engine/` so both the TUI and the web view consume one ordering implementation. This is the second concrete consumer that justifies the move (principle 6). The web layer then walks the ordered nodes and emits a nested `<ul>`/tree.

### GitHub deep-links

Each document links to its GitHub representation, derived from the document's store backend and the repo coordinates (owner/repo/branch derived from the git remote, overridable in `.lazyspec.toml`):

- `filesystem` backend -> blob URL: `/blob/{branch}/{path}`
- `github-issues` backend -> the issue URL
- `github-milestones` backend -> the milestone URL

This is the editing path: a reviewer clicks through to GitHub and uses GitHub's own editor.

## Interfaces

- `lazyspec serve [--port <n>] [--bind <addr>]` @draft -- new CLI subcommand, present only under `--features web`.
- `engine::graph::flatten_forest(...)` @draft -- lifted from `tui/state/graph.rs`; produces the ordered, sibling-sorted node list both TUI and web consume.
- `engine::graph::compare_siblings(...)` @draft -- moved alongside, pure ordering logic.
- `engine::github_url(doc, repo_coords) -> Option<Url>` @draft -- derives the blob/issue/milestone deep-link from a document's backend. Returns `None` (renders no link rather than a broken one) when coordinates can't be resolved: no remote, detached HEAD with no branch, or a backend whose `github_native` mapping (`targets`, `member-of`) doesn't yield a stable URL. Repo coordinates resolve in order: `.lazyspec.toml` `[web]` override, then the `origin` remote; if neither yields owner/repo/branch, deep-links are omitted and `serve` logs a one-line warning at startup.
- `.lazyspec.toml` `[web]` table @draft -- optional overrides for owner/repo/branch; otherwise derived from the git remote.
- `web::server`, `web::routes`, `web::render`, `web::auth` @draft -- internal module surface, not a public API.

## Decisions (ADRs to emit)

- **ADR: web layer as a feature-gated peer module, not a separate crate.** Same binary, `web` feature, `src/web/`. Rationale: engine is already reachable via `lib.rs`; a workspace split is indirection before a second consumer demands it (principle 6).
- **ADR: server-rendered Rust + htmx over a JS SPA.** Keeps one toolchain and one source of truth for the doc model; avoids schema drift from a reimplemented domain.
- **ADR: GitHub OAuth as the read gate, permissions mirror the repo.** Reuses accounts the audience already has; no separate identity system.
- **ADR: lift graph flatten/sort from `tui` into `engine`.** Two consumers (TUI, web) now require one ordering implementation.

## Stories

1. `serve` skeleton: feature flag, axum server, `Arc<Store>` load, `GET /` document list with filters. (Foundational; no auth yet, bind localhost.)
2. Document page: frontmatter header + markdown-to-HTML + `@ref` expansion, reusing engine logic.
3. Search route over the engine search.
4. Lift `flatten_forest`/`compare_siblings` into `engine`; refactor TUI to consume the moved functions (no behavior change); add `GET /graph` tree render.
5. GitHub deep-links: derive repo coordinates from git remote + `.lazyspec.toml` override; `engine::github_url` per backend.
6. GitHub OAuth read gate + membership check + session cookies; flip bind to hosted.

Sequence: 1 precedes all; 2, 3, 4, 5 are independent given 1; 6 gates hosted deployment but not local development of 1-5.

## Risks and tradeoffs

- **Async stack enters the codebase.** tokio/axum are heavy and the project has been deliberately sync. Mitigation: confine entirely to the `web` feature; default builds and the CLI/TUI binary stay async-free.
- **Lifting graph logic risks TUI regressions.** The flatten/sort move must be behavior-preserving. Mitigation: it is a pure refactor with the TUI re-pointed at the moved functions; cover with the existing graph ordering tests.
- **Scope creep toward editing.** A read-only viewer invites "just let them edit one field." The non-goals are load-bearing; editing is a separate RFC with its own write-back/concurrency/auth analysis (Contents API vs PR-based vs server-side clone).
- **Hosting and secret management are real operational cost.** A hosted server with OAuth credentials and repo membership checks is more than a CLI. Mitigation: stories 1-5 are developable and demoable on localhost; only story 6 introduces hosting, so the operational decision can be deferred without blocking the renderer.
- **Tension with the "simple doc tool" scope.** This adds a long-running server mode to a tool positioned as a structured-markdown CLI/TUI. Accepted on the basis that it serves principle 1 (serve structured markdown) to a new audience, reuses the engine wholesale, and adds no surface to default builds.

