---
title: Compile governs globs at store load and answer governing
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-265
- blocks: ITERATION-409
- blocks: ITERATION-413
- blocks: ITERATION-414
- blocks: ITERATION-416
- blocks: ITERATION-417
---

## Objective

The store holds compiled globs per document, and `governing(store, path)` returns every document whose glob matches, with the glob that matched.

## Satisfies

STORY-265 AC4, AC7. Engine half of AC1-AC3.

## Context

- Story + ACs: STORY-265
- Signature, compile point, root semantics: RFC-068 §Design "Frontmatter", §Interfaces `governing`
- Fields and `[governs]` config land in ITERATION-407.
- Touch: `src/engine/store/loader.rs` (compile at load), `src/engine/store.rs` (hold the compiled set, expose `governing`). Engine only -- CONVENTION principle 3.
- Globs resolve relative to `config.governs.root`, not the docs repo. The docs-repo split is the case AC4 names.
- A `governs` entry that will not compile reports which document and which entry (AC7). Find how the loader surfaces a per-document load failure today and use that channel; do not add a second one.
- No index, no cache. A linear walk over documents until there is a measurement -- convention principle 6.

## Tasks

1. Compile each document's `governs` entries with `globset` at store load, keyed to the document.
2. Test-first: an entry that fails to compile produces a load error naming the document path and the entry (AC7).
3. Test-first: `governing` returns one `(&DocMeta, &str)` per matching glob; two documents matching one path both appear with their own globs (AC2); no match returns empty (AC3).
4. Test-first: with `root` pointed outside the docs tree, a path under that root matches and the same relative path under the docs repo does not (AC4).
5. Implement `governing`.

## Out of scope

- `why` or any CLI surface -- ITERATION-409.
- Validation rules over the compiled set -- STORY-267, STORY-268.
- Matching a glob against the filesystem (a directory walk). `governing` matches a path handed to it; the walk is the unowned rule's problem.

## Principles/conventions

`cargo run --quiet -- convention`. Principle 3 on the engine boundary, principle 6 on the absent index.

## Verification

Store load on this repo is unchanged with no pins present. A scratch document pinned `src/engine/**` makes `governing` return it for `src/engine/store.rs` and not for `src/cli/show.rs`.
