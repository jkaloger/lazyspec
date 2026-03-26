---
title: Create spec directory scaffolding
type: iteration
status: accepted
author: agent
date: 2026-03-24
tags: []
related:
- implements: STORY-091
---




## Context

STORY-091 AC `create-spec-scaffolds-document` requires `lazyspec create spec` to produce a flat file at `docs/specs/SPEC-NNN-slug.md`. The loader already handles flat file document types, so no loader changes are needed. The change is entirely in the create command accepting `spec` as a valid document type.

## ACs Addressed

- `create-spec-scaffolds-document`

## Changes

### Task 1: Accept `spec` as a valid document type in create command

ACs addressed: `create-spec-scaffolds-document`

Files:
- Modify: `src/cli/create.rs`
- Modify: `.lazyspec.toml`

What to implement:

Ensure the `spec` type is registered in `.lazyspec.toml` and that `lazyspec create spec` produces a flat file at `docs/specs/SPEC-NNN-slug.md` with `type: spec` frontmatter. Add a `"spec"` branch to `default_template()` for the spec document content.

How to verify:
- `cargo test` passes (no breaking changes to existing config parsing)
- `lazyspec create spec "Test Spec" --author agent` creates `docs/specs/SPEC-NNN-test-spec.md`
- The file has `type: spec` in frontmatter
- `lazyspec show` and `lazyspec list` correctly pick up the new spec
- `lazyspec validate --json` passes

## Test Plan

### test: create spec produces flat file
Invoke `create::run()` with `doc_type = "spec"` using a `TestFixture` that has the spec type configured. Assert that the returned path ends in `.md`, that the file exists, and that the filename follows `SPEC-NNN-slug.md` format. Behavioural, isolated, fast.

### test: created spec has correct frontmatter
Parse the spec file produced by `create::run()` with `DocMeta::parse()`. Assert `type: spec`, `status: draft`, correct title and author. Behavioural, isolated, fast.

### test: created spec loads correctly in store
Create a spec via `create::run()`, then load a `Store` from the same root. Assert the spec appears in `store.all_docs()` with the correct type. Integration-level (trades Fast for Predictive), isolated.

## Notes

The naming pattern `{type}-{n:03}-{title}.md` is shared across all types, including specs.
