---
title: "Document Querying"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [cli, querying, search]
related: []
---

## Summary

The `show`, `list`, and `search` commands form the read path of the CLI. Each delegates to the engine's `Store` for data retrieval and applies its own formatting layer for human or JSON output. All three commands share a common pattern: resolve inputs, query the store, render results.

## Show

The `show` command displays a single document resolved by shorthand ID or literal path. Resolution tries the path first, then falls back to shorthand matching via `resolve_shorthand_or_path`.

@ref src/cli/resolve.rs#resolve_shorthand_or_path

When multiple documents match an unqualified shorthand (e.g. two documents whose canonical name starts with the same prefix), the resolver returns an `Ambiguous` error. The human output lists each matching path and asks the user to be more specific. The JSON output returns a structured `ambiguous_id` error object with the matching paths.

@ref src/engine/store.rs#ResolveError

The display layout renders a title box, then a metadata line (type, status, author), optional tags, and an optional parent reference. A horizontal separator divides the header from the body. If the document has children (folder-based child documents), a "Children" section is appended listing each child's title and qualified shorthand.

@ref src/cli/show.rs#run

Body content is either raw or expanded. When `-e` / `--expand-references` is passed, `@ref` directives in the body are resolved inline via `RefExpander`, with each expansion truncated to `--max-ref-lines` lines (default 25). Without the flag, `@ref` directives appear as-is.

@ref src/engine/refs.rs#RefExpander

JSON output uses `doc_to_json_with_family`, which includes parent and children metadata alongside the standard document fields. The body is added as a string field, respecting the same expand/raw toggle.

@ref src/cli/json.rs#doc_to_json_with_family

## List

The `list` command retrieves all documents from the store, applies optional `--type` and `--status` filters, and sorts by date ascending (with path as tiebreaker).

@ref src/cli/list.rs#run

Filtering is handled by constructing a `Filter` struct with optional `doc_type` and `status` fields. The store's `list` method applies these predicates over its document map.

@ref src/engine/store.rs#Filter

Results are sorted via `DocMeta::sort_by_date`, which orders oldest-first and breaks ties lexicographically by path.

@ref src/engine/document.rs#sort_by_date

For human output, each document renders as a `doc_card` (type badge, title, status, path). Child documents display a parentage annotation ("child of Parent Title") next to their card. JSON output uses `doc_to_json_with_family` for each document, producing an array that includes parent/children metadata.

## Search

The `search` command performs case-insensitive substring matching across three fields in priority order: title, tags, then body. A document appears at most once in results; the first matching field wins.

@ref src/cli/search.rs#run

@ref src/engine/store.rs#search

Title matches use the full title as the snippet. Tag matches use the matched tag. Body matches extract a context window of roughly 40 characters before and after the match position, producing a trimmed snippet displayed between ellipses.

Results are sorted by date (same ordering as `list`). An optional `--type` filter narrows results after the search completes, retaining only documents of the specified type.

@ref src/cli/search.rs#filter_results

Human output renders each result as a `doc_card` annotated with the match field in brackets, followed by the snippet on a second line. When no results match, a "No results" message is printed. JSON output produces an array where each entry includes the standard document fields plus `match_field` and `snippet`.
