---
title: Spec Create Scaffolding and Story Linking
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags: []
related:
- implements: docs/rfcs/RFC-034-spec-certification-and-drift-detection.md
---


## Context

RFC-034 introduces the `spec` document type as a persistent, certifiable contract. The `lazyspec create` command needs to scaffold spec documents. Stories remain standalone documents and link to specs via `implements` relationships -- existing stories that describe a spec's acceptance criteria should be linked to the spec. This story covers the creation scaffolding and the story-linking workflow.

## Acceptance Criteria

### AC: create-spec-scaffolds-document

Given `lazyspec create spec` is invoked with a title
When the spec is created
Then it produces a spec document at `docs/specs/SPEC-NNN-slug.md` (flat file) with `type: spec` frontmatter, or `docs/specs/SPEC-NNN-slug/index.md` (directory) if the user specifies `--dir`

### AC: create-spec-accepts-type

Given `lazyspec create` is invoked
When `spec` is passed as the document type
Then `spec` is accepted as a valid type alongside rfc, story, iteration, adr, and audit

### AC: link-story-to-spec

Given an existing Story document with acceptance criteria
When the user links it to a spec via `lazyspec link <story> implements <spec>`
Then the story's `implements` relationship targets the spec, and the spec's certification and drift detection can collect AC from the linked story

### AC: validate-warns-no-linked-stories

Given a spec document with no stories linked via `implements`
When `lazyspec validate` runs
Then a warning is emitted indicating the spec has no linked stories providing acceptance criteria

## Scope

### In Scope

- `lazyspec create spec` producing a flat file or directory spec document
- `spec` as a valid document type for `lazyspec create`
- Story-to-spec linking via existing `implements` relationship
- `lazyspec validate` warning for specs with no linked stories

### Out of Scope

- ARCH-to-SPEC migration (STORY-088)
- Blob pinning and `@{blob:hash}` syntax (STORY-085)
- Drift detection signals (STORY-087)
- Certification workflow (STORY-086)
- `affects` relationship type and coverage advisories (STORY-089)
- Agent skill updates (STORY-090)
- Spec type recognition in the engine (STORY-088)
