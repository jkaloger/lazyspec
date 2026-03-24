---
title: "Validation Pipeline"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: broken-link-error

Given a document with a `related` entry whose target path does not exist in the store
When `validate_full` runs
Then a `BrokenLink` issue is returned with severity `Error`, containing both the source and target paths

### AC: rejected-parent-error

Given a document linked via a hierarchy relation to a parent with status `Rejected`
When `validate_full` runs
Then a `RejectedParent` issue is returned with severity `Error`

### AC: superseded-parent-warning

Given an `Accepted` document linked via a hierarchy relation to a parent with status `Superseded`
When `validate_full` runs
Then a `SupersededParent` issue is returned with severity `Warning`

### AC: config-driven-parent-child

Given a config with a `ParentChild` rule requiring child type `iteration` to link to parent type `story` via `implements`
When a document of type `iteration` exists without such a link
Then a `MissingParentLink` issue is returned at the severity specified in the config rule

### AC: upward-all-children-accepted

Given a parent document in `Draft` status where every child linked via a hierarchy relation is `Accepted`
When `validate_full` runs
Then an `AllChildrenAccepted` warning is returned listing the parent and all child paths

### AC: upward-partial-accepted

Given a parent in `Draft` status with some `Accepted` children and some non-accepted children
When `validate_full` runs
Then an `UpwardOrphanedAcceptance` warning is returned for each accepted child individually, and no `AllChildrenAccepted` warning is emitted

### AC: duplicate-id-error

Given two documents whose extracted IDs are identical (e.g. both resolve to `RFC-020`)
When `validate_full` runs
Then a `DuplicateId` issue is returned with severity `Error`, listing the shared ID and all conflicting paths in sorted order

### AC: validate-ignore-skip

Given a document with `validate_ignore: true` in its frontmatter
When any checker iterates over the store
Then that document is skipped and no issues are produced where it is the source

### AC: ac-slug-kebab-format

Given a spec `story.md` file containing `### AC: My Bad Slug`
When `validate_full` runs
Then an `InvalidAcSlug` warning is returned because the slug does not match the kebab-case regex `^[a-z0-9]+(-[a-z0-9]+)*$`

### AC: ac-slug-duplicate

Given a spec `story.md` file containing two `### AC:` headings with the same slug
When `validate_full` runs
Then an `InvalidAcSlug` warning is returned with reason "duplicate AC slug" for the second occurrence

### AC: ref-count-ceiling

Given a spec `index.md` containing 20 distinct `@ref` targets and a config with `ref_count_ceiling` set to 15
When `validate_full` runs
Then a `RefCountExceeded` warning is returned reporting the count (20) and ceiling (15)

### AC: cross-module-refs

Given a spec `index.md` whose `@ref` directives reference files in 4 or more distinct module prefixes (first two path segments)
When `validate_full` runs
Then a `CrossModuleRefs` warning is returned reporting the module count

### AC: orphan-ref-warning

Given a spec `index.md` containing `@ref src/nonexistent.rs#Foo` where `src/nonexistent.rs` does not exist on disk
When `validate_full` runs
Then an `OrphanRef` warning is returned with the missing path

### AC: severity-partitioning

Given checkers that return a mix of `(Severity::Error, issue)` and `(Severity::Warning, issue)` pairs
When `validate_full` collects results
Then errors appear in `ValidationResult.errors` and warnings appear in `ValidationResult.warnings`, with no cross-contamination
