---
title: Issue body format and parsing
type: iteration
status: accepted
author: unknown
date: 2026-03-27
tags: []
related:
- implements: STORY-101
---




## Goal

Implement the issue body format module for RFC-037's `github-issues` store backend. This covers serialization of lazyspec documents into the HTML comment frontmatter format, deserialization back into `DocMeta`, round-trip fidelity, status reconstruction from issue open/closed state, and error handling for malformed input.

The module is self-contained: no GitHub API calls, no caching, no TUI changes. It produces and consumes strings.

## Task Breakdown

### 1. Add `src/engine/issue_body.rs` module

Create the module and wire it into `src/engine.rs`. Define the public API surface:

- `IssueContext` struct holding the fields that come from GitHub primitives (title, labels, is_open) rather than the issue body
- `serialize(doc: &DocMeta, body: &str) -> String` that produces the HTML comment block + markdown body
- `deserialize(issue_body: &str, ctx: &IssueContext) -> Result<(DocMeta, String)>` that parses the comment block and reconstructs the document

The HTML comment format from RFC-037:

```html
<!-- lazyspec
---
author: agent-7
date: 2026-03-27
related:
- implements: STORY-075
---
-->
```

Only `author`, `date`, `related`, and non-lifecycle `status` go in the comment. Title, tags, type, and lifecycle status come from `IssueContext`.

### 2. Implement `serialize`

Build the YAML frontmatter string from `DocMeta` fields that belong in the comment block. Wrap it in `<!-- lazyspec\n---\n...\n---\n-->`. Append the markdown body after a blank line.

Reuse the existing `Relation` serialization format from `src/engine/document.rs` for the `related` field. The `status` field is only included when it is a non-lifecycle value (rejected, superseded) per the RFC-037 status mapping table.

### 3. Implement HTML comment extraction

Write a parser that finds the `<!-- lazyspec` marker, extracts the YAML between `---` delimiters inside the comment, and returns the remaining body text.

This is analogous to `split_frontmatter` in `src/engine/document.rs` but targets the HTML comment wrapper instead of bare `---` delimiters. The parser should handle:

- Leading/trailing whitespace around the comment block
- Missing closing `-->` (error)
- Missing or empty YAML block (error)
- Body text before and after the comment (only text after counts as body)

### 4. Implement `deserialize` with status reconstruction

Parse the extracted YAML into the comment-only fields (author, date, related, status). Combine with `IssueContext` to build a full `DocMeta`:

- `title` from `IssueContext.title`
- `tags` from `IssueContext.labels` (filtering out `lazyspec:*` prefixed labels)
- `doc_type` from the `lazyspec:{type}` label in `IssueContext.labels`
- `status` reconstruction per RFC-037 rules:
  - Frontmatter `status` takes precedence when present
  - `is_open && no status` -> `draft`
  - `!is_open && no status` -> `complete`

The `complete` status does not exist on the current `Status` enum in `src/engine/document.rs`. Either extend the enum or map to the closest existing value. Check what RFC-037 intends and align.

### 5. Extend `Status` enum if needed

The RFC-037 status mapping references `complete`, `in-progress`, and other values not in the current `Status` enum (which has: Draft, Review, Accepted, Rejected, Superseded). Determine whether to:

- Add new variants (`Complete`, `InProgress`) to the existing enum
- Or treat `complete` as `Accepted` for now and leave a TODO

The choice depends on whether other parts of the codebase (validation, TUI status display) can handle new variants without breaking. Check `src/engine/validation.rs` and `src/tui/views/colors.rs` for status-dependent logic.

### 6. Error handling for malformed input

Define error types for parse failures:

- `MissingComment` -- no `<!-- lazyspec` marker found
- `MalformedComment` -- marker found but no valid YAML delimiters or closing `-->`
- `InvalidFrontmatter` -- YAML present but fails to deserialize (missing required fields, bad date format, unknown relation type)

Return these as structured errors from `deserialize`, not panics. The caller (future store implementation) will decide whether to surface them as warnings or hard failures.

### 7. Round-trip fidelity tests

Write tests that serialize a `DocMeta` + body, then deserialize the output, and assert equality on all fields. Cover:

- Minimal document (no relations, no tags, draft status)
- Document with multiple relations
- Document with non-lifecycle status (rejected, superseded)
- Document with lifecycle status (draft, accepted) where status is _not_ written to the comment
- Body containing HTML comments (should not confuse the parser)
- Body containing `---` lines (should not confuse the YAML extraction)

### 8. Edge case and error tests

- Missing `<!-- lazyspec` marker returns `MissingComment`
- `<!-- lazyspec` without closing `-->` returns `MalformedComment`
- Empty YAML block (just delimiters, no fields) returns `InvalidFrontmatter`
- Unknown fields in YAML are ignored (forward compatibility)
- Extra whitespace around comment block is tolerated
- Multiple `<!-- lazyspec` blocks: first one wins, rest treated as body

## Test Plan

- Unit tests for `serialize` covering all field combinations
- Unit tests for the HTML comment extraction parser
- Unit tests for `deserialize` with various `IssueContext` combinations
- Unit tests for status reconstruction (all 7 rows of the RFC-037 status table)
- Round-trip property: `deserialize(serialize(doc, body), ctx)` recovers the original `DocMeta` and body
- Error case tests for each malformed input variant
- Regression test: body text containing `<!-- ... -->` HTML comments that are not lazyspec markers

## Notes

The `Status` enum extension (task 5) may touch shared code. If it cascades into too many files, stub it with a TODO and limit this iteration to the format module itself. The status reconstruction logic should still be correct at the type level even if the enum is not yet extended.

`DocMeta.path` and `DocMeta.id` are not meaningful for github-issues documents in the same way as filesystem documents. The deserialize function should set `path` to an empty `PathBuf` and `id` to an empty string; the caller (store layer) assigns these based on the issue number mapping.
