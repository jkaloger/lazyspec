---
title: 'Document page: frontmatter header, markdown-to-HTML, @ref expansion'
type: story
status: complete
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
---## Context

RFC-052 specifies a per-document web page. This story renders `GET /doc/{id}`: a structured frontmatter header, the markdown body rendered to HTML, and `@ref` directives expanded inline. It reuses the engine wholesale -- `pulldown-cmark` (already a dependency) for markdown-to-HTML, and the existing expand-references logic that backs `show --expand-references`. No new domain logic; the web layer only renders what the engine already produces. Depends on STORY-176 (the `serve` skeleton and `Arc<Store>`).

## Acceptance Criteria

- **Given** a running `serve` instance and a valid document id
  **When** a client requests `GET /doc/{id}`
  **Then** the response renders the document's frontmatter (type, status, author, date, tags, relations) as a structured header above the body.

- **Given** a document whose body contains markdown
  **When** the document page renders
  **Then** the body is converted to HTML via the engine's `pulldown-cmark` pipeline.

- **Given** a document body containing `@ref` directives
  **When** the page renders
  **Then** each `@ref` is expanded inline using the same engine logic that backs `show --expand-references`.

- **Given** an unknown document id
  **When** a client requests `GET /doc/{id}`
  **Then** the server responds with HTTP 404 and a rendered not-found page (a handled response, never a 500 or a dropped connection).

- **Given** the document list from STORY-176
  **When** the user clicks a document row
  **Then** they navigate to that document's `/doc/{id}` page.

## Scope

### In Scope

- `GET /doc/{id}` route and `askama` template.
- Frontmatter header rendering from the engine document model.
- Markdown-to-HTML via the engine's existing `pulldown-cmark` usage.
- `@ref` expansion reusing the engine's expand-references logic.
- 404 handling for unknown ids.

### Out of Scope

- GitHub deep-link on the page (STORY-180).
- Relationship graph render (STORY-179).
- Search (STORY-178).
- Any write/edit affordance (out of RFC scope entirely).
- Mermaid/diagram rendering of fenced code blocks. The page renders plain markdown to HTML; the static JS diagram widget is a separate follow-up and no AC here depends on it.
