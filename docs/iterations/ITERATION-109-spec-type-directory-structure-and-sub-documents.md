---
title: Spec type, directory structure, and sub-documents
type: iteration
status: accepted
author: agent
date: 2026-03-24
tags: []
related:
- implements: docs/stories/STORY-088-spec-document-type-and-migration.md
---



## Context

STORY-088 introduces the `spec` document type to lazyspec, evolving from the existing `arch` type. This first iteration covers the foundational work: registering the type, establishing its directory structure, loading `story.md` as a relationship-inheriting sub-document, and validating the `### AC: <slug>` heading format.

The existing subdirectory loader (`store/loader.rs`) already handles `index.md` + sibling `.md` files for `arch` documents. The main new behaviours are: a `SPEC` constant on `DocType`, the `spec` type entry in `.lazyspec.toml`, relationship inheritance for sub-documents, and a validation checker for AC slug format in `story.md` files.

## ACs Addressed

- `spec-type-recognised` -- engine recognises `type: spec` as valid
- `spec-directory-structure` -- `docs/specs/SPEC-NNN-slug/index.md` with optional `story.md`
- `story-sub-document` -- `story.md` treated as sub-document inheriting parent relationships
- `ac-slug-format` -- `### AC: <slug>` heading format validated in `story.md`

## Changes

### Task 1: Register `spec` document type

ACs addressed: `spec-type-recognised`

Files:
- Modify: `src/engine/document.rs`
- Modify: `.lazyspec.toml`

What to implement:

Add `pub const SPEC: &str = "spec";` to `DocType` alongside the existing `RFC`, `STORY`, `ITERATION`, `ADR` constants. Add a `[[types]]` entry in `.lazyspec.toml`:

```toml
[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
icon = "📋"
```

The subdirectory loader already handles directory-based document types with `index.md`, so no loader changes are needed for basic type recognition.

How to verify:
- `cargo test` passes
- `lazyspec create spec "Test Spec" --author agent --json` succeeds and creates a file under `docs/specs/`
- `lazyspec list --json` includes the new spec document with `type: spec`

### Task 2: Relationship inheritance for sub-documents

ACs addressed: `story-sub-document`

Files:
- Modify: `src/engine/store/loader.rs`
- Modify: `src/engine/store.rs`

What to implement:

Currently, sub-documents (children of an `index.md` parent) carry their own independent `related` field. The AC requires that `story.md` (and potentially any sub-document of a spec) inherits the parent's relationships when the engine resolves links.

Add a method `effective_relations` (or similar) to `Store` that, for a given document, returns its own relations merged with its parent's relations if it has a parent. This keeps the stored data clean (sub-documents don't duplicate frontmatter) while providing inherited relations at query time.

The `build_links` method in `store.rs` constructs `forward_links` and `reverse_links` from each doc's `related` field. Extend this to also propagate the parent's relations to its children. For each child in `parent_of`, copy the parent's forward links to the child's forward link set (and update reverse links accordingly). This ensures `story.md` appears in relationship queries (e.g. `lazyspec context`) as if it had the parent's relations.

How to verify:
- Create a spec with `index.md` containing `related: [{implements: docs/rfcs/RFC-034...}]` and a `story.md` with no `related` field
- `lazyspec show <spec-id>/story --json` should include the inherited `implements` relation
- `lazyspec context <spec-id> --json` should include the story sub-document in the chain

### Task 3: AC slug format validation

ACs addressed: `ac-slug-format`

Files:
- Modify: `src/engine/validation.rs`

What to implement:

Add a new `Checker` implementation, `AcSlugFormatRule`, that fires for sub-documents named `story.md` whose parent has `type: spec`. The checker parses the document body looking for `### AC:` headings and validates:

1. Every `### AC:` heading has a slug (not empty after the colon)
2. The slug matches `[a-z0-9]+(-[a-z0-9]+)*` (lowercase kebab-case)
3. No duplicate slugs within the same `story.md`

This requires reading the file content during validation. The existing checkers operate on `DocMeta` (frontmatter only). This checker will need access to the file body. Check how `RefExpander` or other components access file content -- likely through `FileSystem::read_to_string`. The checker can receive a reference to the `FileSystem` trait or the validation runner can pass file content alongside metadata.

Add a new `ValidationIssue` variant, e.g. `InvalidAcSlug { path, slug, reason }`.

How to verify:
- A `story.md` with `### AC: valid-slug` passes validation
- A `story.md` with `### AC: ` (empty slug) produces a validation warning
- A `story.md` with `### AC: CamelCase` produces a validation warning
- A `story.md` with duplicate `### AC: same-slug` entries produces a validation warning
- A non-spec sub-document with arbitrary `###` headings does NOT trigger this validation

## Test Plan

### test: spec type loads from directory structure
Create a `TestFixture` with `.lazyspec.toml` containing the spec type definition and a `docs/specs/SPEC-001-test/index.md` file. Verify `store.all_docs()` includes it with `type: spec`. Behavioural, isolated, fast.

### test: spec story.md loaded as sub-document
Create a fixture with `docs/specs/SPEC-001-test/index.md` and `docs/specs/SPEC-001-test/story.md`. Verify `store.children_of()` returns `story.md` and `store.parent_of()` returns the index path. Behavioural, isolated, fast.

### test: story.md inherits parent relationships
Create a fixture where `index.md` has `related: [{implements: docs/rfcs/RFC-001.md}]` and `story.md` has no `related` field. Verify that `store.forward_links` for the story path includes the inherited `implements` link. Tradeoff: tests internal link structure rather than CLI output, but is more specific about the inheritance mechanism.

### test: valid AC slugs pass validation
Create a spec `story.md` with well-formed `### AC: kebab-case` headings. Run validation. Verify no `InvalidAcSlug` issues are emitted.

### test: invalid AC slugs produce warnings
Create a spec `story.md` with empty slugs, uppercase slugs, and duplicates. Run validation. Verify the correct `InvalidAcSlug` issues are emitted with appropriate reasons.

### test: non-spec sub-documents skip AC validation
Create an `arch` or `rfc` sub-document with arbitrary headings. Run validation. Verify no AC-related issues are emitted.

## Notes

The relationship inheritance in Task 2 is the most significant design decision. Two approaches were considered:

1. Propagate at link-build time (chosen): copy parent relations to children in `build_links`. Simple, consistent with how the rest of the system queries links.
2. Resolve lazily at query time: keep links as-is, add a method that walks up to the parent when queried. More flexible but adds complexity to every callsite that queries relations.

Option 1 was chosen because it keeps the query path simple and matches the existing pattern where `forward_links`/`reverse_links` are the single source of truth after loading.
