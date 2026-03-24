---
title: "Document Model and Organization"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: frontmatter-parsed-into-docmeta

Given a markdown file with valid YAML frontmatter delimited by `---`
When `DocMeta::parse` is called on the file content
Then the title, doc_type, status, author, date, tags, and related fields are populated, and path/id are left empty (assigned later by the loader)

### AC: missing-frontmatter-returns-error

Given a markdown file without opening `---` delimiters
When `split_frontmatter` is called on the content
Then an error "no frontmatter found" is returned

### AC: unclosed-frontmatter-returns-error

Given a markdown file with an opening `---` but no closing `---`
When `split_frontmatter` is called on the content
Then an error "no closing frontmatter delimiter" is returned

### AC: doctype-lowercased-on-construction

Given a frontmatter with `type: RFC` (uppercase)
When the document is parsed
Then `doc_type` contains the value `"rfc"` (lowercased)

### AC: status-rejects-unknown-values

Given a frontmatter with `status: archived` (not a recognized variant)
When the document is parsed
Then an error "unknown status: archived" is returned

### AC: relation-parsed-from-yaml-mapping

Given a frontmatter with `related: [{ implements: "docs/rfcs/RFC-001.md" }]`
When the document is parsed
Then `related` contains one `Relation` with `rel_type: Implements` and `target: "docs/rfcs/RFC-001.md"`

### AC: related-to-accepts-hyphenated-and-spaced

Given a relation key of `"related-to"` or `"related to"`
When the relation is parsed
Then both forms resolve to `RelationType::RelatedTo`

### AC: flat-file-id-extraction

Given a flat file at `docs/rfcs/RFC-001-my-feature.md`
When the document is loaded
Then its `id` is set to `"RFC-001"`

### AC: index-file-id-from-folder

Given a folder-based document at `docs/stories/STORY-001-user-auth/index.md`
When the document is loaded
Then its `id` is derived from the folder name `STORY-001-user-auth`, yielding `"STORY-001"`

### AC: child-documents-discovered

Given a folder `docs/rfcs/RFC-014-nested/` containing `index.md`, `threat-model.md`, and `appendix.md`
When the type directory is loaded
Then `index.md` is the parent, and both `threat-model.md` and `appendix.md` are tracked as children in the `children` and `parent_of` maps

### AC: virtual-parent-synthesized-without-index

Given a folder `docs/stories/STORY-050-example/` containing `part-a.md` and `part-b.md` but no `index.md`
When the type directory is loaded
Then a virtual parent is created at path `stories/STORY-050-example/.virtual` with `virtual_doc: true`, title derived from the folder name, and type matching the configured type

### AC: virtual-parent-status-all-accepted

Given a folder without `index.md` where every child document has `status: accepted`
When the virtual parent is synthesized
Then the virtual parent's status is `Accepted`

### AC: virtual-parent-status-mixed

Given a folder without `index.md` where at least one child has a status other than `accepted`
When the virtual parent is synthesized
Then the virtual parent's status is `Draft`

### AC: rewrite-frontmatter-preserves-body

Given a document with frontmatter and body content
When `rewrite_frontmatter` is called with a mutation that changes the status field
Then the body content after the closing `---` is preserved exactly, and the YAML section reflects the mutation

### AC: sort-by-date-ascending-with-path-tiebreak

Given two documents with the same date but different paths
When sorted using `DocMeta::sort_by_date`
Then the document with the lexicographically smaller path appears first
