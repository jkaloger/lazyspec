---
title: "Subfolder Document Support"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-06
tags: []
---


## Summary

Allow documents to live in subfolders with an `index.md` entrypoint, so authors can co-locate supporting assets (images, diagrams, supplementary notes) alongside their documents.

## Problem

Today, lazyspec discovers documents by scanning each configured directory (e.g. `docs/rfcs/`) for flat `.md` files. There's no way to associate supporting files with a document. Authors who want to include diagrams or supplementary material have to put them somewhere disconnected from the document itself, or use external hosting.

## Design

### Folder-based documents

In addition to flat markdown files, lazyspec should recognise subfolders that contain an `index.md` file as documents. The folder name follows the same naming convention as flat files:

```
docs/rfcs/
  RFC-001-simple-feature.md              # flat file (unchanged)
  RFC-010-subfolder-document-support/    # folder-based document
    index.md                             # entrypoint -- frontmatter lives here
    architecture.png                     # supporting asset (opaque to lazyspec)
    notes.md                             # supplementary file (opaque to lazyspec)
```

### Discovery changes

The current discovery logic in `Store::load()` iterates entries in each doc directory and filters for `.md` extension. The change:

1. For each entry in a doc directory, check if it is a file ending in `.md` (existing behavior) **or** a subdirectory containing an `index.md` file
2. If it's a subdirectory with `index.md`, parse `index.md` as the document
3. The document's path becomes `docs/rfcs/RFC-010-subfolder-document-support/index.md` (the full relative path including `index.md`)

```
@ref src/engine/store.rs#Store::load
```

### What stays the same

- Frontmatter format, validation rules, and all CLI commands work identically
- The `index.md` file is parsed exactly like any other document file
- Relationships reference the full path (including `/index.md`)
- Shorthand resolution works against the folder name (e.g. `RFC-010` matches `RFC-010-subfolder-document-support/index.md`)

### What is explicitly out of scope

- Lazyspec has no awareness of non-`index.md` files in the folder. They are opaque supporting assets.
- No recursive discovery beyond one level of nesting.
- No special rendering or aggregation of folder contents.
- Relationships between `index.md` and sibling files in the same folder (future work).

### Edge cases

- A folder without `index.md` is silently ignored (not an error).
- A folder and a flat file with the same prefix (e.g. `RFC-010.md` and `RFC-010-something/index.md`) are both valid, distinct documents.
- Shorthand resolution matches against the folder name's stem, consistent with how flat file stems are matched today.

## Stories

1. **Subfolder document discovery** -- extend `Store::load()` to discover `index.md` inside subfolders, with shorthand resolution and relationship support working correctly.
2. **CLI subfolder creation** -- extend `lazyspec create` to optionally create a folder-based document instead of a flat file.
