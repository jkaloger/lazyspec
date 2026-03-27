---
title: Issue body format and parsing
type: story
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-037-github-issues-document-store.md
---

## Context

GitHub Issues store documents with an HTML comment block containing YAML frontmatter for fields that don't map to GitHub primitives (author, date, related, non-lifecycle status). The visible markdown below is the document body. Title maps to issue title, tags to labels, type to a `lazyspec:{type}` label, and status is derived from open/closed state plus frontmatter overrides.

## Acceptance Criteria

### AC: Serializing lazyspec document to issue body

Given a lazyspec document with title, body, frontmatter fields (author, date, related, status), tags, and type
When the document is serialized to an issue body format
Then an HTML comment block containing YAML frontmatter is produced, followed by the document body as markdown

### AC: Parsing issue body back into lazyspec document

Given an issue body with HTML comment frontmatter and markdown content, issue title, labels, and open/closed state
When parsing the issue body
Then the frontmatter is extracted from the HTML comment, title reconstructed from issue title, tags from labels, status reconstructed from open/closed state and frontmatter

### AC: Round-trip fidelity

Given a lazyspec document serialized to an issue body then parsed back
When comparing the original and re-parsed documents
Then all fields (title, body, frontmatter, tags, type, status) are identical

### AC: Malformed or missing HTML comment handling

Given an issue body with missing or malformed HTML comment frontmatter
When parsing
Then a validation error is returned identifying the format problem

### AC: Status reconstruction logic

Given various combinations of issue open/closed state and frontmatter status
When reconstructing document status
Then: open with no frontmatter status maps to draft, closed with no frontmatter status maps to complete, and frontmatter status takes precedence over derived status

## Scope

### In Scope

- Serialization of lazyspec document to HTML comment + markdown format
- Parsing issue body to extract frontmatter and reconstruct document
- Round-trip fidelity testing
- Validation of HTML comment format
- Status reconstruction from open/closed state and frontmatter

### Out of Scope

- Actual GitHub API calls
- Caching of parsed documents
- TUI integration for issue body display
- Label management or custom field handling
