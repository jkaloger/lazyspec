---
title: "Document Store"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: store-loads-all-configured-types

Given a project root with multiple type directories configured in `Config`
When `Store::load_with_fs` is called
Then every `.md` file in each existing type directory is parsed and indexed by its relative path, and directories that do not exist are silently skipped

### AC: parse-errors-collected-not-fatal

Given a type directory contains a markdown file with invalid frontmatter
When `Store::load_with_fs` runs
Then the file's relative path and error string are appended to `parse_errors`, loading continues for remaining files, and valid documents in the same directory load successfully

### AC: child-discovery-with-index

Given a subdirectory within a type directory contains `index.md` and additional `.md` files
When the store loads that subdirectory
Then `index.md` is parsed as the parent document, each additional `.md` file is parsed as a child, and the `children` and `parent_of` maps reflect the relationship

### AC: virtual-parent-synthesis

Given a subdirectory within a type directory contains `.md` files but no `index.md`
When the store loads that subdirectory
Then a virtual parent is created with `virtual_doc: true`, a `.virtual` sentinel path, a title derived from the folder name, and status `accepted` only if all children are accepted (otherwise `draft`)

### AC: forward-and-reverse-links-built

Given documents have `related` entries in their frontmatter
When the store finishes loading
Then `forward_links_for` returns the document's outgoing relationships and `reverse_links_for` returns incoming relationships, both keyed by `(RelationType, PathBuf)`

### AC: parent-links-propagated-to-children

Given a parent document declares `related` links and has child documents
When the store finishes loading
Then each child's forward links include the parent's links, and the corresponding targets have reverse links pointing back to each child

### AC: unqualified-shorthand-resolves-single

Given a non-child document with filename starting with `RFC-001`
When `resolve_shorthand("RFC-001")` is called
Then it returns that document's `DocMeta`

### AC: unqualified-shorthand-ambiguous

Given two non-child documents whose canonical filenames both start with `STORY-0`
When `resolve_shorthand("STORY-0")` is called
Then it returns `ResolveError::Ambiguous` containing both paths

### AC: qualified-shorthand-resolves-child

Given a parent folder `RFC-014-nested` contains a child `threat-model.md`
When `resolve_shorthand("RFC-014/threat-model")` is called
Then it returns the child document's `DocMeta`

### AC: unqualified-shorthand-excludes-children

Given child documents exist in the store
When `resolve_shorthand` is called with an unqualified ID that matches a child's filename
Then the child is not considered a candidate and the call returns `NotFound` or matches only non-child documents

### AC: search-priority-cascade

Given a document whose title contains "migration" and another whose body contains "migration" but title does not
When `search("migration")` is called
Then the title-matched document appears with `match_field: "title"` and the body-matched document appears with `match_field: "body"`, each appearing at most once

### AC: filter-conjunctive-application

Given documents of various types, statuses, and tags
When `Store::list` is called with a `Filter` specifying both `doc_type` and `status`
Then only documents matching both constraints are returned

### AC: reload-file-upserts-and-rebuilds

Given a document already exists in the store
When `reload_file` is called with its path and the file has been modified on disk
Then the document is re-parsed, the docs map is updated, any prior parse error for that path is cleared, and the link graph is fully rebuilt

### AC: reload-file-removes-deleted

Given a document exists in the store
When `reload_file` is called and the file no longer exists on disk
Then the document is removed from the docs map and the link graph is rebuilt

### AC: id-extraction-from-filename

Given a file named `RFC-001-some-title.md`
When `extract_id` processes its path
Then the returned id is `RFC-001`, and for `index.md` or `.virtual` files the parent folder name is used instead
