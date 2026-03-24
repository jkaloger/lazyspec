---
title: "Document Querying"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: show-resolves-by-path

Given a document exists at a known relative path
When `lazyspec show <path>` is run
Then the document's title box, metadata line, and body are printed to stdout

### AC: show-resolves-by-shorthand

Given a document whose canonical name starts with "RFC-001"
When `lazyspec show RFC-001` is run
Then the matching document is displayed as if resolved by path

### AC: show-ambiguous-id-human

Given two documents whose canonical names both start with the same prefix
When `lazyspec show <prefix>` is run without `--json`
Then stderr lists each matching path and advises the user to specify the full path

### AC: show-ambiguous-id-json

Given two documents whose canonical names both start with the same prefix
When `lazyspec show <prefix> --json` is run
Then the output is a JSON object with `"error": "ambiguous_id"` and an `ambiguous_matches` array

### AC: show-expand-references

Given a document body contains `@ref` directives
When `lazyspec show <id> -e` is run
Then each `@ref` directive is replaced inline with the referenced content, truncated to `--max-ref-lines` (default 25)

### AC: show-children-section

Given a folder-based parent document has child documents
When `lazyspec show <parent-id>` is run
Then a "Children" section appears after the body listing each child's title and qualified shorthand

### AC: show-parent-annotation

Given a child document exists within a folder-based parent
When `lazyspec show <child-id>` is run
Then the metadata section includes a "Parent" line with the parent's title and path

### AC: list-filter-by-type-and-status

Given a project contains documents of various types and statuses
When `lazyspec list <type> --status <status>` is run
Then only documents matching both the type and status are returned

### AC: list-sorted-by-date

Given a project contains documents with different dates
When `lazyspec list` is run
Then results are sorted oldest-first, with path as tiebreaker for equal dates

### AC: list-child-parentage-display

Given a child document appears in list results
When the human output renders
Then the child's card is annotated with "(child of Parent Title)"

### AC: search-matches-title-tag-body

Given documents exist where one matches by title, another by tag, and another by body
When `lazyspec search <query>` is run
Then all three documents appear in results with their respective `match_field` values ("title", "tag", "body")

### AC: search-first-match-wins

Given a document's title and body both contain the query term
When `lazyspec search <query>` is run
Then the document appears once with `match_field` set to "title" (the first checked field)

### AC: search-body-snippet-context

Given a document matches only in its body content
When `lazyspec search <query>` is run
Then the snippet contains approximately 40 characters of context on each side of the match

### AC: search-type-filter

Given search results include documents of multiple types
When `lazyspec search <query> --type <type>` is run
Then only results matching the specified document type are returned
