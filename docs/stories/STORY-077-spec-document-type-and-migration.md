---
title: Spec Document Type and Migration
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags:
- certification
- specs
- migration
related:
- implements: docs/rfcs/RFC-031-spec-certification-and-drift-detection.md
---


## Context

Lazyspec's architecture documents (`arch` type) are static. Once accepted, nothing connects them to the codebase or detects when reality diverges from what they describe. RFC-031 introduces the `spec` document type as the replacement: a persistent, certifiable contract whose scope is defined by `@ref` directives and whose behavioural claims live in a `story.md` sub-document with stable acceptance criteria identifiers.

This story covers introducing the `spec` type to the engine, establishing its directory structure and AC format, migrating existing ARCH documents, and adding validation rules that keep spec scope honest.

## Acceptance Criteria

### AC: spec-type-recognised

Given the lazyspec engine's document type registry
When a document with `type: spec` is loaded
Then the engine recognises it as a valid document type alongside rfc, story, iteration, adr, audit, and arch

### AC: spec-directory-structure

Given a spec document is created or migrated
When it is stored on disk
Then it follows the structure `docs/specs/SPEC-NNN-slug/index.md` with an optional `story.md` sub-document in the same directory

### AC: story-sub-document

Given a spec directory contains both `index.md` and `story.md`
When the engine loads the spec
Then `story.md` is treated as a sub-document of the spec, both with `type: spec`, and it inherits the parent spec's relationships

### AC: ac-slug-format

Given a `story.md` file for a spec
When acceptance criteria are defined
Then each criterion uses the heading format `### AC: <slug>` where slug is a stable, human-readable identifier (not a sequential number)

### AC: arch-to-spec-migration

Given the existing architecture documents ARCH-001 through ARCH-005 in `docs/architecture/`
When the migration is performed
Then they become SPEC-001 through SPEC-005 in `docs/specs/`, with `type: spec` in frontmatter and the new directory structure preserved

### AC: validate-ref-count-ceiling

Given a spec document with `@ref` directives in its `index.md`
When `lazyspec validate` runs and the spec has more than the configured ceiling (default 15) of `@ref` targets
Then a warning is emitted indicating the spec may be too broad and should be split

### AC: validate-cross-module-advisory

Given a spec document whose `@ref` directives target symbols in more than 3 distinct modules
When `lazyspec validate` runs
Then an advisory warning is emitted suggesting the spec covers a cross-cutting concern and may benefit from splitting

### AC: validate-orphan-ref

Given a spec document with a `@ref` directive targeting a symbol
When `lazyspec validate` runs and that symbol cannot be found at HEAD
Then a validation warning is emitted identifying the orphaned ref

### AC: ref-ceiling-configurable

Given a `.lazyspec.toml` configuration file with a custom `ref_count_ceiling` value
When `lazyspec validate` evaluates a spec's ref count
Then the configured ceiling is used instead of the default 15

## Scope

### In Scope

- `spec` as a new document type in the engine's type registry
- Directory layout: `docs/specs/SPEC-NNN-slug/index.md` + `story.md`
- `### AC: <slug>` heading format for acceptance criteria in `story.md`
- `story.md` as a sub-document of the spec (both `type: spec`), inheriting relationships
- Migration of ARCH-001 through ARCH-005 to SPEC-001 through SPEC-005
- `lazyspec validate` warnings: ref count ceiling (default 15, configurable), cross-module advisory (>3 modules), orphan ref check

### Out of Scope

- Blob pinning and `@{blob:hash}` syntax (Story 2)
- Semantic hashing / AST normalization (Story 2)
- Drift detection signals and `lazyspec drift` command (Story 3)
- Certification workflow and `lazyspec certify` (Story 4)
- `affects` relationship type and coverage advisories (Story 5)
- `certified_by`, `certified_date`, `story_hashes` frontmatter fields (Story 4)
