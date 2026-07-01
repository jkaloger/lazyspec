---
title: 'Document page: frontmatter header, markdown-to-HTML, @ref expansion'
type: iteration
status: complete
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-177
---

## Objective

Render `GET /doc/{id}`: a structured frontmatter header, the markdown body converted to HTML, and `@ref` directives expanded inline; 404 for unknown ids.

## Context

- Story + ACs: STORY-177 (parent).
- RFC: docs/rfcs/RFC-052-read-only-web-view-for-lazyspec-docs.md (Routes, Layering).
- Conventions: docs/convention/CONVENTION.md, DICTUM-004-testing (test-first), DICTUM-005-tech-stack, DICTUM-003-module-structure. Layering rule: `web` imports only from `engine`, never `cli`/`tui`.
- Skeleton this builds on: STORY-176 established `web::server`, `web::routes`, the `askama` setup, and the shared `Arc<Store>` under the `web` feature.
- Touch: src/web/routes.rs (add `GET /doc/{id}` handler + register route), src/web/render.rs (markdown-to-HTML helper + frontmatter view model; new if absent), templates for the document page and a not-found page (alongside the STORY-176 list template), src/web.rs (module wiring if a new submodule is added).
- Engine reuse (do not reimplement): `Store::get` for the `DocMeta` (type, status, author, date, tags, related); `Store::get_body_expanded(path, max_ref_lines, fs)` for `@ref` expansion -- the same path backing `show --expand-references` (see src/cli/show.rs); `Store::parent_of` / `Store::children_of` for relations. Resolve shorthand ids the way src/cli/resolve.rs does.
- Markdown-to-HTML: `pulldown-cmark` is already a dependency but is only wired for terminal rendering today (src/tui/content/gfm). This slice adds the HTML path via `pulldown_cmark::html::push_html` over the expanded body. Expand refs first, then render the result to HTML.

## Satisfies

STORY-177 AC1 (frontmatter header), AC2 (markdown-to-HTML), AC3 (`@ref` expansion), AC4 (404 + not-found page), AC5 (list row links to `/doc/{id}`).

## Tasks

1. Add a frontmatter view model in src/web/render.rs populated from `DocMeta` (type, status, author, date, tags, relations) plus parent/children from the store.
2. Add a markdown-to-HTML helper in src/web/render.rs using `pulldown_cmark::html::push_html`, fed the ref-expanded body from `Store::get_body_expanded`.
3. Add the `askama` document-page template (frontmatter header block above the rendered HTML body) and a not-found template.
4. Test-first: handler tests for the success path (header fields present, body HTML rendered, a `@ref` expanded inline) and the unknown-id path (HTTP 404, rendered not-found page, never 500).
5. Implement the `GET /doc/{id}` handler in src/web/routes.rs: resolve id -> 404 on miss; otherwise render the page. Register the route.
6. Link each row in the STORY-176 `GET /` list template to `/doc/{id}`.

## Out of scope

- GitHub deep-link on the page -> STORY-180.
- Relationship graph render -> STORY-179.
- Search -> STORY-178.
- Mermaid/diagram rendering of fenced code blocks (plain markdown-to-HTML only; the static JS widget is a separate follow-up).
- Any write/edit affordance (out of RFC scope).
- Auth / hosted bind -> STORY-181.

## Verification

- `@ref path#symbol` in a body renders as expanded inline content on the page, not the literal `@ref` text.
- `GET /doc/UNKNOWN-999` returns status 404 with the not-found page body, not a 500 or dropped connection.
- A row click on `GET /` lands on the corresponding `/doc/{id}` page.

