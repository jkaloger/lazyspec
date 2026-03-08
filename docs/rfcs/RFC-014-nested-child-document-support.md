---
title: Nested child document support
type: rfc
status: accepted
author: jkaloger
date: 2026-03-07
tags: []
related:
- related-to: docs/rfcs/RFC-010-subfolder-document-support.md
---



## Summary

Extend folder-based documents so that all markdown files within a document folder are discovered as child documents of the parent. Today, `Store::load()` recognises subfolders with `index.md` but treats every other file in the folder as opaque. This RFC promotes those sibling markdown files to first-class documents with their own frontmatter, validation, relationships, and addressability.

## Problem

RFC-010 introduced subfolder documents with `index.md` as the sole entrypoint. Authors can co-locate assets, but lazyspec has no awareness of additional markdown files in the folder. In practice, authors want to split large documents into sections (e.g. an RFC with a separate threat model, or a story with detailed acceptance criteria in a companion file). These companion files are invisible to search, validation, context, and the TUI.

## Design

### Folder structure

A folder-based document can contain any number of `.md` files alongside `index.md`:

```
docs/rfcs/
  RFC-001-simple.md                          # flat file (unchanged)
  RFC-014-nested-child-document-support/     # folder-based document
    index.md                                 # parent document (frontmatter here)
    threat-model.md                          # child document
    alternatives.md                          # child document
```

### Parent resolution

When a folder contains `index.md`, that file is the parent document. All other `.md` files in the folder are children.

When a folder does not contain `index.md`, the folder itself acts as a virtual parent:
- Title is derived from the folder name (e.g. `RFC-014-nested-child-document-support` becomes "Nested child document support")
- Type is inferred from the folder prefix
- Status is `draft` unless every child document has `accepted` status, in which case the virtual parent is `accepted`

```
@ref src/engine/store.rs#Store::load
```

### Child documents are full documents

Each child `.md` file must have valid frontmatter. Children are parsed, validated, and stored in the `Store` just like any other document. The parent-child relationship is implicit from the folder structure rather than declared in frontmatter `related` fields.

Children can have their own `related` fields pointing to external documents. The folder containment relationship is separate from the `Implements`/`RelatedTo` relationship types used elsewhere.

### Discovery changes

`Store::load()` currently handles two cases per directory entry: flat `.md` file, or subdirectory with `index.md`. The change adds a third pass when a subdirectory is found:

1. Check for `index.md` -- if present, parse it as the parent (existing behavior)
2. Scan for all other `.md` files in the subdirectory and parse each as a child document
3. If no `index.md` exists but `.md` files are found, synthesise a virtual parent

No recursive discovery beyond one level of nesting. Subfolders within a document folder are ignored.

```
@ref src/engine/store.rs#Store::load
```

### Shorthand resolution

Children are addressable via qualified shorthand: `RFC-014/threat-model`. The resolution logic checks for a `/` separator and, if present, resolves the prefix against parent documents and the suffix against children within that parent's folder.

Unqualified shorthand (e.g. `threat-model`) does not resolve to children, avoiding namespace collisions when multiple folders contain files with the same name.

```
@ref src/engine/store.rs#Store::resolve_shorthand
```

### Parent-child link representation

A new internal relationship concept is needed to distinguish folder containment from `Implements`/`RelatedTo`. Options:

- Add a `ChildOf` / `ParentOf` variant to `RelationType` and populate it during discovery
- Or maintain a separate `children: HashMap<PathBuf, Vec<PathBuf>>` index on Store

The second approach keeps the existing relationship model focused on authored links while folder structure is a discovery-time concern. The `children` index maps parent path to child paths, and a corresponding `parent_of: HashMap<PathBuf, PathBuf>` provides the reverse lookup.

```
@draft Store {
    docs: HashMap<PathBuf, DocMeta>,
    children: HashMap<PathBuf, Vec<PathBuf>>,   // parent -> children
    parent_of: HashMap<PathBuf, PathBuf>,        // child -> parent
}
```

### CLI behavior

**`show <id>`** -- When the target is a parent with children, display the parent's content followed by a "Children" section listing each child's title and qualified shorthand. When the target is a child, display that child's content normally plus a note indicating its parent.

**`context <id>`** -- Children appear as relationships in the context chain, consistent with how `Implements` links are shown today. The context command does not concatenate child content.

**`list`** -- Children appear in list results as normal documents. They are not filtered or hidden.

**`search <query>`** -- A match on a child returns that child document. The parent and siblings are not included unless they also match. This is consistent with how search works for independent documents.

**`validate`** -- Children are validated independently since they have their own frontmatter. Virtual parents are validated for structural correctness (all children parseable, no conflicting types).

**`create`** -- Out of scope for this RFC. A separate story will address creating child documents and folder-based documents via the CLI.

### TUI behavior

Parent documents with children render as expandable tree nodes in the document list. This reuses the depth-based rendering already implemented for graph mode (`GraphNode` with depth and ASCII connectors). Expanding a parent reveals its children as indented items.

```
@ref src/tui/ui.rs
@ref src/tui/app.rs#App::rebuild_graph
```

### Edge cases

- A folder with only non-markdown files and no `index.md` is silently ignored (existing behavior, unchanged).
- A folder with `.md` files but no `index.md` creates a virtual parent. The virtual parent is not written to disk.
- If a child document's type (from frontmatter) conflicts with the parent's type, validation emits a warning but does not reject. Authors may intentionally nest an ADR inside an RFC folder.
- Shorthand collision: if `RFC-014/notes` and `RFC-015/notes` both exist, qualified shorthand disambiguates. Unqualified `notes` resolves to neither.

## Stories

1. **Child document discovery and engine support** -- extend `Store::load()` to discover child `.md` files in subfolders, build the `children`/`parent_of` indexes, support virtual parents, and handle qualified shorthand resolution.

2. **CLI child document support** -- update `show`, `context`, `list`, `search`, and `validate` commands to handle parent-child relationships from folder structure.

3. **TUI expandable tree nodes** -- render parent documents with children as expandable nodes in the document list view, reusing depth-based rendering.

4. **CLI create for folder-based documents** -- extend `lazyspec create` to support creating folder-based documents and adding children to existing folders. (Open issues: UX for converting a flat file to a folder, naming conventions for children.)
