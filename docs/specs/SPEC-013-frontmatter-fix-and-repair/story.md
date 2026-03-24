---
title: "Frontmatter Fix and Repair"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: missing-fields-filled-with-defaults

Given a document is missing one or more required frontmatter fields (title, type, status, author, date, tags)
When the user runs `lazyspec fix <path>`
Then the missing fields are added with contextual defaults (status: "draft", tags: [], date: today, author from git config, title from filename, type from directory) and existing fields are preserved unchanged

### AC: no-frontmatter-delimiters-handled

Given a document has no YAML frontmatter delimiters (`---`)
When the user runs `lazyspec fix <path>`
Then a complete frontmatter block is generated with all required fields and the original content is preserved as the body

### AC: dry-run-reports-without-writing

Given a document with missing frontmatter fields or numbering conflicts
When the user runs `lazyspec fix --dry-run`
Then the output describes what changes would be made but no files on disk are modified

### AC: json-output-matches-fix-output-shape

Given documents that need field fixes or conflict resolution
When the user runs `lazyspec fix --json`
Then the output is valid JSON matching the `FixOutput` shape with `field_fixes` and `conflict_fixes` arrays, where each entry contains path, fields_added/old_path/new_path, and written fields

### AC: fix-all-parse-errors-with-no-paths

Given multiple documents have parse errors in the store
When the user runs `lazyspec fix` with no path arguments
Then all documents with parse errors are fixed in a single pass

### AC: type-inferred-from-directory

Given a document in a configured type directory (e.g. `rfcs/`) is missing the `type` field
When the user runs `lazyspec fix <path>`
Then the `type` field is set based on the directory-to-type mapping in configuration

### AC: duplicate-id-detected-and-renumbered

Given two or more documents share the same extracted ID (e.g. two `RFC-020` files)
When `lazyspec fix` is run
Then the document with the earliest `date` frontmatter (mtime as tiebreaker) keeps its number and the others are renumbered to the next available ID for that type

### AC: subfolder-document-directory-renamed

Given a conflicting document uses the subfolder layout (e.g. `RFC-020-bar/index.md`)
When `lazyspec fix` renumbers it
Then the entire directory is renamed to reflect the new ID and all files within it move with the directory

### AC: frontmatter-title-updated-on-renumber

Given a renumbered document's frontmatter title contains the old ID prefix
When the rename completes
Then the old ID prefix in the title is replaced with the new one

### AC: related-references-cascaded

Given document A has a `related` frontmatter entry targeting document B's path
When document B is renumbered by the fix command
Then document A's `related` entry is rewritten to document B's new path

### AC: body-ref-directives-cascaded

Given document A contains a `@ref` body directive pointing at document B's path
When document B is renumbered by the fix command
Then the `@ref` directive in document A's body is rewritten to document B's new path

### AC: external-references-warned

Given files outside managed document directories reference a renamed document
When `lazyspec fix renumber` completes
Then a warning is emitted listing each external file, line number, and the old document name that could not be auto-updated

### AC: resolve-shorthand-ambiguity-error

Given two documents share the same numeric ID prefix
When `resolve_shorthand` is called with that ID
Then it returns an error listing all matching document paths instead of silently picking one

### AC: duplicate-id-validation-diagnostic

Given two or more documents share the same extracted ID
When `lazyspec validate` is run
Then a `DuplicateId` error is reported listing the conflicting ID and all document paths, and documents with `validate_ignore: true` are excluded from grouping
