---
title: Enriched templates and init materialization
type: story
status: accepted
author: jkaloger
date: 2026-06-21
tags: []
related:
- implements: RFC-048
---

## Context

Each document type has a template that decides what sections a new doc carries. Today those templates are bare markdown with `{key}` substitution (title, author, date, type), one file per type at `.lazyspec/templates/{type}.md`, falling back to defaults baked into the binary. Sections are stubbed with flat `TODO:` lines that say nothing about what belongs there.

That carries no methodology. An author (human or agent) sees a heading and a `TODO` and has to guess intent. The per-type template should instead carry how to author that type well, as data that travels with the doc.

This story enriches the template format. Each template gains an `<!-- intent: ... -->` header comment stating the purpose of the type, and each section gains an `<!-- guidance: ... -->` comment describing what belongs under that heading. These replace the bare `TODO:` lines. Comments are plain markdown, so they render invisibly in the final document while still guiding whoever fills it in.

It also makes `init` materialize the default templates to disk. Today `init` creates the templates directory empty and the defaults live only in the binary. After this change `init` writes a template file per default type, so authors and the future `/configure-type` meta-skill have real files to edit. The existing on-disk-overrides-embedded behaviour is preserved: an edited template file still wins over the embedded default.

## Acceptance Criteria

- **Given** a project with default templates on disk
  **When** a new document of that type is created
  **Then** the created file contains the per-section `<!-- guidance: ... -->` comments and the `<!-- intent: ... -->` header from the template.

- **Given** a created document containing intent and guidance comments
  **When** the document is rendered for display
  **Then** the comments do not appear in the rendered output, leaving only the visible markdown.

- **Given** an enriched template containing `{title}`, `{author}`, `{date}`, and `{type}` placeholders
  **When** a document is created from it
  **Then** each placeholder is replaced with its value and the surrounding intent and guidance comments are left intact.

- **Given** a project that has not yet been initialized
  **When** `lazyspec init` runs
  **Then** a template file is written to the templates directory for each default type, rather than the directory being left empty.

- **Given** an on-disk template file that has been edited away from the embedded default
  **When** a document of that type is created
  **Then** the document is rendered from the edited on-disk template, not the embedded default.

- **Given** the set of default document types (rfc, story, iteration, and the rest)
  **When** their materialized templates are inspected
  **Then** each carries an `<!-- intent: ... -->` header and an `<!-- guidance: ... -->` comment per section.

## Scope

### In Scope

- Enrich the per-type template format with an `<!-- intent: ... -->` header comment and an `<!-- guidance: ... -->` comment per section, replacing bare `TODO:` lines.
- Keep templates plain markdown with the existing `{key}` substitution (title, author, date, type) intact.
- Make `lazyspec init` materialize the default templates to disk, one file per default type.
- Preserve on-disk-overrides-embedded: an edited template file wins over the embedded default.
- Ship enriched default templates for the default types (rfc, story, iteration, etc.).

### Out of Scope

- The generic verb skills that consume templates at runtime (STORY-147).
- The config axes, including the `intent` field on `TypeDef` (STORY-145). The template's `<!-- intent -->` is template content and is distinct from the config `intent` field.
- The config-editing CLI (STORY-146).
- The `/configure-type` meta-skill that authors per-type templates (STORY-149).
