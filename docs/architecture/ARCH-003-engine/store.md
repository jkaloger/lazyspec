---
title: "Store"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine, store]
related:
  - related-to: "docs/stories/STORY-001-document-model-and-store.md"
  - related-to: "docs/stories/STORY-040-child-document-discovery.md"
  - related-to: "docs/stories/STORY-044-store-parse-error-collection.md"
---

# Store

The `Store` is the central data structure. It's an in-memory index of all documents
in the project, built by scanning configured type directories on disk.

@ref src/engine/store.rs#Store

## Loading

`Store::load(root, config)` walks each type directory (non-recursive, one level deep)
and handles three cases per entry:

1. **Markdown file** -- parse frontmatter, register document
2. **Directory with index.md** -- parse index as parent, scan children
3. **Directory without index.md** -- create virtual parent, scan children

All paths are stored relative to the project root for portability.

See [STORY-040: Child document discovery](../../stories/STORY-040-child-document-discovery.md) for the
nested document scanning logic and [STORY-044: Store parse error collection](../../stories/STORY-044-store-parse-error-collection.md)
for error tolerance during loading.

## Link Index

After loading all documents, the Store builds forward and reverse link indices
from the `related` field in each document's frontmatter:

```d2
direction: right

doc_a: "RFC-001" {
  related: "implements: STORY-001"
}

forward: "forward_links" {
  shape: cylinder
  entry: "RFC-001 -> [(implements, STORY-001)]"
}

reverse: "reverse_links" {
  shape: cylinder
  entry: "STORY-001 -> [(implements, RFC-001)]"
}

doc_a -> forward: "build"
doc_a -> reverse: "build"
```

## Hot Reload

`reload_file(root, relative_path)` re-parses a single document and rebuilds the
link index. Used by the TUI's file watcher to keep the store current without a
full reload. If the file no longer exists, it's removed from the index.

@ref src/engine/store.rs#reload_file

## Shorthand Resolution

`resolve_shorthand(id)` converts human-friendly IDs to full paths:

- `RFC-001` -- matches any non-child document whose filename starts with "RFC-001"
- `STORY-001/child-005` -- qualified: finds parent first, then child within it
- Returns `ResolveError::Ambiguous` when multiple documents match

@ref src/engine/store.rs#ResolveError

## Search

`search(query)` does case-insensitive substring matching across title, tags, and
body content. Results include the match field and a context snippet. Priority:
title matches first, then tags, then body.

@ref src/engine/store.rs#SearchResult
