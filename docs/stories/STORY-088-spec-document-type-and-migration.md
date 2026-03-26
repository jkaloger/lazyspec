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
- implements: docs/rfcs/RFC-034-spec-certification-and-drift-detection.md
---



## Context

Lazyspec's architecture documents (`arch` type) are static. Once accepted, nothing connects them to the codebase or detects when reality diverges from what they describe. RFC-034 introduces the `spec` document type as the replacement: a persistent, certifiable contract whose scope is defined by `@ref` directives and whose behavioural claims are defined by acceptance criteria in linked Story documents.

This story covers introducing the `spec` type to the engine, establishing its document structure, migrating existing ARCH documents, and adding validation rules that keep spec scope honest.

## Acceptance Criteria

### AC: spec-type-recognised

Given the lazyspec engine's document type registry
When a document with `type: spec` is loaded
Then the engine recognises it as a valid document type alongside rfc, story, iteration, adr, audit, and arch

### AC: spec-document-structure

Given a spec document is created or migrated
When it is stored on disk
Then it follows the flat file layout (`docs/specs/SPEC-NNN-slug.md`)

### AC: spec-linked-stories

Given a spec document exists
When Stories with `implements` relationships targeting the spec are loaded
Then the engine recognises the linked stories as the spec's acceptance criteria source for certification and drift detection

### AC: ac-slug-format

Given a Story linked to a spec
When acceptance criteria are defined
Then each criterion uses the heading format `### AC: <slug>` where slug is a stable, human-readable identifier (not a sequential number)

### AC: arch-to-spec-migration

Given the existing architecture documents ARCH-001 through ARCH-005 in `docs/architecture/`
When the migration is performed
Then they become SPEC-001 through SPEC-005 in `docs/specs/`, with `type: spec` in frontmatter and the flat file layout

### AC: validate-ref-count-ceiling

Given a spec document with `@ref` directives
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
- Document layout: flat file (`docs/specs/SPEC-NNN-slug.md`)
- `### AC: <slug>` heading format for acceptance criteria in linked Story documents
- Stories link to specs via `implements` relationships (existing relationship model)
- Migration of ARCH-001 through ARCH-005 to SPEC-001 through SPEC-005
- `lazyspec validate` warnings: ref count ceiling (default 15, configurable), cross-module advisory (>3 modules), orphan ref check, specs with no linked stories

### Out of Scope

- Blob pinning and `@{blob:hash}` syntax (STORY-085)
- Semantic hashing / AST normalization (STORY-085)
- Drift detection signals and `lazyspec drift` command (STORY-087)
- Certification workflow and `lazyspec certify` (STORY-086)
- `affects` relationship type and coverage advisories (STORY-089)
- `certified_by`, `certified_date`, `story_hashes` frontmatter fields (STORY-086)
