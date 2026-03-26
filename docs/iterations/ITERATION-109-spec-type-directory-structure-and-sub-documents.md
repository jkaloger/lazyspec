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

STORY-088 introduces the `spec` document type to lazyspec, evolving from the existing `arch` type. This first iteration covers the foundational work: registering the type, establishing its flat file layout, and validating the `### AC: <slug>` heading format in linked Story documents.

The main new behaviours are: a `SPEC` constant on `DocType`, the `spec` type entry in `.lazyspec.toml`, and a validation checker for AC slug format in linked Story documents.

## ACs Addressed

- `spec-type-recognised` -- engine recognises `type: spec` as valid
- `spec-document-structure` -- `docs/specs/SPEC-NNN-slug.md` flat file layout
- `ac-slug-format` -- `### AC: <slug>` heading format validated in linked Story documents

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

The loader already handles flat file document types, so no loader changes are needed for basic type recognition.

How to verify:
- `cargo test` passes
- `lazyspec create spec "Test Spec" --author agent --json` succeeds and creates a file under `docs/specs/`
- `lazyspec list --json` includes the new spec document with `type: spec`

### Task 2: Story-to-spec linking via implements relationship

ACs addressed: `spec-linked-stories`

Files:
- Modify: `src/engine/store.rs`

What to implement:

Stories are standalone flat files that link to specs via `implements` relationships in their frontmatter. The engine already supports relationship resolution through `build_links`. No sub-document or relationship inheritance logic is needed -- stories declare their own `implements` relationship targeting the spec path.

How to verify:
- Create a spec at `docs/specs/SPEC-001-test.md` and a story with `related: [{implements: docs/specs/SPEC-001-test.md}]`
- `lazyspec show <story-id> --json` should include the `implements` relation
- `lazyspec context <spec-id> --json` should include the linked story in the chain

### Task 3: AC slug format validation

ACs addressed: `ac-slug-format`

Files:
- Modify: `src/engine/validation.rs`

What to implement:

Add a new `Checker` implementation, `AcSlugFormatRule`, that fires for Story documents linked to a spec via `implements` relationships. The checker parses the document body looking for `### AC:` headings and validates:

1. Every `### AC:` heading has a slug (not empty after the colon)
2. The slug matches `[a-z0-9]+(-[a-z0-9]+)*` (lowercase kebab-case)
3. No duplicate slugs within the same Story document

This requires reading the file content during validation. The existing checkers operate on `DocMeta` (frontmatter only). This checker will need access to the file body. Check how `RefExpander` or other components access file content -- likely through `FileSystem::read_to_string`. The checker can receive a reference to the `FileSystem` trait or the validation runner can pass file content alongside metadata.

Add a new `ValidationIssue` variant, e.g. `InvalidAcSlug { path, slug, reason }`.

Note: the checker identifies linked stories by querying the relationship graph for documents with `implements` pointing to a `type: spec` document, rather than relying on filesystem co-location.

How to verify:
- A Story linked to a spec with `### AC: valid-slug` passes validation
- A Story linked to a spec with `### AC: ` (empty slug) produces a validation warning
- A Story linked to a spec with `### AC: CamelCase` produces a validation warning
- A Story linked to a spec with duplicate `### AC: same-slug` entries produces a validation warning
- A Story not linked to any spec with arbitrary `###` headings does NOT trigger this validation

## Test Plan

### test: spec type loads from flat file
Create a `TestFixture` with `.lazyspec.toml` containing the spec type definition and a `docs/specs/SPEC-001-test.md` file. Verify `store.all_docs()` includes it with `type: spec`. Behavioural, isolated, fast.

### test: story links to spec via implements relationship
Create a fixture with `docs/specs/SPEC-001-test.md` and a story with `related: [{implements: docs/specs/SPEC-001-test.md}]`. Verify that `store.forward_links` for the story path includes the `implements` link to the spec. Behavioural, isolated, fast.

### test: valid AC slugs pass validation
Create a Story document linked to a spec with well-formed `### AC: kebab-case` headings. Run validation. Verify no `InvalidAcSlug` issues are emitted.

### test: invalid AC slugs produce warnings
Create a Story document linked to a spec with empty slugs, uppercase slugs, and duplicates. Run validation. Verify the correct `InvalidAcSlug` issues are emitted with appropriate reasons.

### test: non-spec-linked stories skip AC validation
Create a Story document not linked to any spec with arbitrary headings. Run validation. Verify no AC-related issues are emitted.

## Notes

Stories link to specs via `implements` relationships, following the same relationship model used throughout lazyspec. No sub-document or inheritance mechanism is needed -- stories are standalone flat files that declare their own relationships.
