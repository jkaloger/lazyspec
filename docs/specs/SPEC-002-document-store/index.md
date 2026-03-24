---
title: "Document Store"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related:
  - related-to: "docs/stories/STORY-001-document-model-and-store.md"
  - related-to: "docs/stories/STORY-040-child-document-discovery.md"
  - related-to: "docs/stories/STORY-044-store-parse-error-collection.md"
  - related-to: "docs/stories/STORY-068-universal-id-resolution.md"
---

## Summary

The `Store` (@ref src/engine/store.rs#Store) is the in-memory index of every document in a lazyspec project. It scans configured type directories, parses frontmatter from markdown files, builds a bidirectional link graph from `related` fields, and tracks parent-child relationships for nested documents. All document paths are stored relative to the project root.

The Store does not read document bodies at load time. Body content is fetched on demand through `get_body_raw` and `get_body_expanded`, the latter delegating to `RefExpander` for inline reference expansion.

## FileSystem Abstraction

All disk I/O goes through the `FileSystem` trait (@ref src/engine/fs.rs#FileSystem), which declares seven methods: `read_to_string`, `write`, `rename`, `read_dir`, `exists`, `create_dir_all`, and `is_dir`. The production implementation is `RealFileSystem` (@ref src/engine/fs.rs#RealFileSystem), a thin wrapper over `std::fs`. Tests inject an `InMemoryFileSystem` to avoid touching disk.

`Store::load` delegates to `RealFileSystem` by default. `Store::load_with_fs` accepts any `&dyn FileSystem`, which is the entry point used by tests and by callers that need a custom filesystem.

## Loading

`Store::load_with_fs` iterates the `documents.types` list from `Config`. For each type definition, it joins the type's `dir` field to the project root and skips the directory if it does not exist. Within each type directory, the loader (@ref src/engine/store/loader.rs#load_type_directory) handles entries as follows:

- A `.md` file at the top level is parsed via `DocMeta::parse`. On success, its path is stored relative to the root and its `id` is extracted with `extract_id`. On failure, the path and error message are collected into a `ParseError` (@ref src/engine/store.rs#ParseError) and loading continues.
- A subdirectory containing `index.md` treats the index as the parent document. All other `.md` files in that subdirectory become children, tracked in the `children` and `parent_of` maps.
- A subdirectory without `index.md` triggers virtual parent synthesis. The loader creates a `DocMeta` with `virtual_doc: true`, a title derived from the folder name (stripping the type prefix and sqid), and a `.virtual` sentinel path. The virtual parent's status is `accepted` only if every child is accepted; otherwise it is `draft`.

Subdirectories nested inside a document folder are ignored. The loader does not recurse beyond one level.

## ID Extraction

`extract_id` (@ref src/engine/store.rs#extract_id) assigns a short identifier to each document based on its filename. For `index.md` or `.virtual` files, the folder name is used instead. The helper `extract_id_from_name` (@ref src/engine/store.rs#extract_id_from_name) splits on hyphens and returns the prefix up to and including the first segment that contains a non-uppercase character (e.g. `RFC-001-some-title` yields `RFC-001`).

When a file lives inside a parent folder whose name itself has an extractable prefix (i.e. the parent is a document folder), `extract_id` returns the bare stem, since the child's identity is scoped to its parent.

## Bidirectional Link Graph

After all documents are loaded, `build_links` (@ref src/engine/store/links.rs#build_links) iterates every document's `related` field and populates two `HashMap`s: `forward_links` (source to list of `(RelationType, target)`) and `reverse_links` (target to list of `(RelationType, source)`). Both directions are available through `forward_links_for` and `reverse_links_for`.

`propagate_parent_links` (@ref src/engine/store/links.rs#propagate_parent_links) then copies each parent's forward links onto its children, adding corresponding reverse entries. This means a child inherits the relationships declared by its parent without needing to redeclare them.

`related_to` returns the union of forward and reverse links for a path, while `referenced_by` returns only reverse links.

## Shorthand Resolution

`resolve_shorthand` (@ref src/engine/store.rs#resolve_shorthand) converts a human-friendly identifier into a `DocMeta` reference. It supports two forms:

- Unqualified (e.g. `RFC-001`): matches any non-child document whose canonical filename starts with the given string. If zero documents match, it returns `ResolveError::NotFound`. If more than one matches, it returns `ResolveError::Ambiguous` (@ref src/engine/store.rs#ResolveError) with the list of conflicting paths. Child documents are excluded from unqualified resolution to avoid accidental matches.
- Qualified (e.g. `STORY-040/threat-model`): splits on `/`, resolves the parent portion among non-child documents, then finds a child whose file stem starts with the child portion.

The `canonical_name` function normalises `index.md` and `.virtual` files to their parent folder name before comparison.

## Filtering and Listing

`Filter` (@ref src/engine/store.rs#Filter) holds optional constraints on `doc_type`, `status`, and `tag`. `Store::list` applies these conjunctively: a document must satisfy every present filter field. All three fields default to `None`, so an empty filter returns all documents.

## Search

`Store::search` (@ref src/engine/store.rs#SearchResult) performs case-insensitive substring matching with a priority cascade: title, then tags, then body. A document appears at most once in the results, under the first field that matches. Body matches include a context snippet of roughly 80 characters centred on the match position. Results are sorted by date via `DocMeta::sort_by_date`.

## Hot Reload

`reload_file` re-parses a single document by relative path. If the file no longer exists on disk, it is removed from the docs map. If parsing succeeds, the document is upserted and any prior parse error for that path is cleared. If parsing fails, the document is removed and a new `ParseError` is recorded. In all cases, the link graph is fully rebuilt via `rebuild_links` (@ref src/engine/store/links.rs#rebuild_links), which clears both link maps, re-derives them from all docs, and re-runs parent link propagation.

`remove_file` drops a document by path and rebuilds links.
