---
title: Story-to-Spec Migration and Create Scaffolding
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags: []
related:
- implements: docs/rfcs/RFC-034-spec-certification-and-drift-detection.md
---


## Context

RFC-034 introduces the `spec` document type as a persistent, certifiable contract. Specs use a directory structure (`docs/specs/SPEC-NNN-slug/index.md` + `story.md`) rather than a single file. The `lazyspec create` command needs to scaffold this structure. Additionally, the RFC supersedes standalone Story documents: their AC migrate into spec `story.md` sub-documents, their `@ref` directives move into spec `index.md`, and iterations that implemented the Story re-link to the spec. This story covers both the creation scaffolding and the Story-to-spec migration path.

## Acceptance Criteria

### AC: create-spec-scaffolds-directory

Given `lazyspec create spec` is invoked with a title
When the spec is created
Then it produces a `docs/specs/SPEC-NNN-slug/` directory containing both `index.md` (with `type: spec` frontmatter) and `story.md` (with `type: spec` frontmatter, empty AC template)

### AC: story-ac-migration

Given an existing Story document with given/when/then acceptance criteria
When the Story is migrated to a spec
Then its AC are moved into the target spec's `story.md` using `### AC: <slug>` format, stripping any `@ref` directives

### AC: story-ref-migration

Given an existing Story document containing `@ref` directives
When the Story is migrated to a spec
Then test `@ref` directives move into the spec's `index.md`, not into `story.md`

### AC: iteration-relink

Given iterations that `implements` a Story being migrated
When the Story is migrated to a spec
Then those iterations' `implements` relationships are updated to target the spec

### AC: story-superseded-status

Given a Story whose AC have been migrated to a spec
When the migration is complete
Then the Story's status is set to `superseded`

### AC: validate-warns-superseded-links

Given an iteration linked via `implements` to a Story with `status: superseded`
When `lazyspec validate` runs
Then a warning is emitted prompting re-linking to the spec

### AC: migrate-command

Given the `lazyspec migrate` command (or equivalent)
When invoked for Story-to-spec migration
Then it performs AC migration, ref migration, iteration re-linking, and status update as a single operation

## Scope

### In Scope

- `lazyspec create spec` producing the `SPEC-NNN-slug/` directory with `index.md` and `story.md`
- Story AC migration into spec `story.md` using `### AC: <slug>` format
- Story `@ref` directive migration into spec `index.md`
- Iteration relationship re-linking from Story to spec
- `status: superseded` on migrated Stories
- `lazyspec validate` warning for iterations linked to superseded Stories
- `lazyspec migrate` command (or equivalent) for the Story-to-spec transition

### Out of Scope

- ARCH-to-SPEC migration (STORY-088)
- Blob pinning and `@{blob:hash}` syntax (STORY-085)
- Drift detection signals (STORY-087)
- Certification workflow (STORY-086)
- `affects` relationship type and coverage advisories (STORY-089)
- Agent skill updates (STORY-090)
- spec type recognition in the engine (STORY-088)
