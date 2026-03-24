---
title: "Document Model and Organization"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Overview

Every document in lazyspec is a markdown file with YAML frontmatter. The frontmatter is parsed into a @ref src/engine/document.rs#DocMeta struct that carries all metadata the engine needs: path, title, type, status, author, date, tags, relations, and two internal flags (`validate_ignore` and `virtual_doc`). The `id` field is derived from the filename at load time, never stored in frontmatter.

## Frontmatter Parsing

The @ref src/engine/document.rs#split_frontmatter function extracts YAML from the leading `---` delimiters. It trims leading whitespace, expects an opening `---`, and searches for a closing `\n---`. If either delimiter is missing, it returns an error. The YAML string and the remaining body are returned as a tuple.

`DocMeta::parse` deserializes the YAML through a @ref src/engine/document.rs#RawFrontmatter intermediate struct. `RawFrontmatter` uses serde to map the `type` field (via `#[serde(rename)]`) to `DocType`, the `validate-ignore` field (via `#[serde(rename, default)]`) to a bool, and the `related` field to a `Vec<serde_yaml::Value>` that is parsed in a second pass by `parse_relation`. The path and id are not set during parsing; they are assigned by the loader after the fact.

The @ref src/engine/document.rs#rewrite_frontmatter function provides in-place mutation of a document's frontmatter. It reads the file, splits it, deserializes the YAML into a `serde_yaml::Value`, applies a caller-supplied closure, re-serializes, and writes the file back. The body is preserved verbatim.

## DocType

@ref src/engine/document.rs#DocType is a newtype around `String`, always lowercased on construction and deserialization. It defines string constants for the built-in types (`rfc`, `story`, `iteration`, `adr`, `spec`) but accepts any string. The set of valid types is determined by the project's `.lazyspec.toml` configuration, not by the Rust type system. `FromStr` succeeds for any input; validation against configured types happens elsewhere.

## Status

@ref src/engine/document.rs#Status is a closed enum with five variants: `Draft`, `Review`, `Accepted`, `Rejected`, and `Superseded`. Deserialization is case-insensitive via a custom `FromStr` implementation that lowercases input before matching. Unrecognized values produce an error. Status controls virtual parent synthesis and appears in listing/filtering output.

## Relations

@ref src/engine/document.rs#RelationType is an enum with four variants: `Implements`, `Supersedes`, `Blocks`, and `RelatedTo`. The `FromStr` implementation accepts `"related-to"` and `"related to"` as synonyms. Each relation in frontmatter is a single-key YAML mapping (e.g. `- implements: "docs/rfcs/RFC-001.md"`), parsed by `parse_relation` into a @ref src/engine/document.rs#Relation struct containing a `RelationType` and a target path string.

## Document ID Derivation

IDs are extracted from filenames by @ref src/engine/store.rs#extract_id, not stored in frontmatter. The algorithm handles three cases:

- For `index.md` or `.virtual` files, the ID comes from the parent folder name.
- For child documents (files inside a folder whose name itself has a type prefix), the file stem is used as-is. This preserves short names like `threat-model` rather than truncating.
- For flat files, `extract_id_from_name` splits the stem on `-` and returns segments up to and including the first segment that is not all-uppercase (e.g. `RFC-001-my-feature` yields `RFC-001`).

## Directory Layout

The loader in @ref src/engine/store/loader.rs#load_type_directory iterates each configured type directory. Top-level `.md` files are parsed directly (flat layout). Subdirectories trigger `load_subdirectory`, which checks for `index.md`.

When `index.md` exists, it becomes the parent document. All other `.md` files in that directory (excluding `index.md` itself and subdirectories) are loaded as children. The @ref src/engine/store/loader.rs#load_subdirectory function populates two maps: `children` maps a parent path to its child paths, and `parent_of` maps each child path back to its parent. Non-markdown files are ignored. Subdirectories within document folders are also ignored, preventing recursive nesting.

## Virtual Parent Synthesis

When a subdirectory has no `index.md`, the loader synthesizes a virtual parent. The virtual document's path is set to `<folder_relative>/.virtual`, its title is derived from the folder name via `title_from_folder_name` (which strips the type prefix and sqid, then title-cases the first word), and its type is set to the containing type directory's configured name.

The virtual parent's status is computed from its children: if every child has `Status::Accepted`, the virtual parent is `Accepted`; otherwise it is `Draft`. The `virtual_doc` flag is set to `true`, and the document is inserted into the docs map but never written to disk. The `date` is set to the current UTC date.

## Sorting

@ref src/engine/document.rs#sort_by_date orders documents by date ascending, with path as a tiebreaker when dates are equal. This is a static method on `DocMeta` intended for use with `Vec::sort_by`.
