---
title: Validate-Ignore Flag
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags:
- validation
- migration
related:
- implements: RFC-008
---




## Context

When migrating an existing project to lazyspec, legacy documentation often has incomplete or inconsistent relationships. These documents trigger validation warnings (e.g. unlinked iterations, orphaned acceptance) that are correct but unhelpful -- the user knows these docs predate the workflow and doesn't intend to fix them.

There is currently no way to suppress validation for specific documents. This story adds a `validate-ignore` frontmatter flag that tells the validation engine to skip checks involving the marked document.

## Acceptance Criteria

### AC1: Frontmatter field is parsed

- **Given** a document with `validate-ignore: true` in its frontmatter
  **When** the document is loaded by the engine
  **Then** the `validate-ignore` flag is available on the document model

### AC2: Ignored document skipped as source

- **Given** a document with `validate-ignore: true` that has a broken `implements` link
  **When** `lazyspec validate` is run
  **Then** no error is reported for that broken link

### AC3: Ignored document skipped as target

- **Given** an accepted story that implements a draft RFC
  **And** the story has `validate-ignore: true`
  **When** `lazyspec validate` is run
  **Then** no "accepted story but parent RFC not accepted" warning is reported for that story

### AC4: Non-ignored documents still validated

- **Given** a project with one ignored document and one non-ignored document, both with validation issues
  **When** `lazyspec validate` is run
  **Then** warnings and errors for the non-ignored document are still reported

### AC5: Status output reflects ignore flag

- **Given** a document with `validate-ignore: true`
  **When** `lazyspec status --json` is run
  **Then** the document's JSON representation includes `"validate_ignore": true`

### AC6: Default is false

- **Given** a document without a `validate-ignore` field in its frontmatter
  **When** the document is loaded
  **Then** the flag defaults to `false` and validation runs normally

## Scope

### In Scope

- Parsing `validate-ignore` from frontmatter
- Skipping all validation checks where the ignored document is involved (as source or target)
- Surfacing the flag in JSON output (`status`, `show`)
- Updating the `create` and `update` commands to support the field

### Out of Scope

- Per-rule suppression (e.g. ignoring only specific validation rules)
- A `.lazyspecignore` file or pattern-based ignoring
- TUI indicators for ignored documents
- Bulk-marking documents as ignored
