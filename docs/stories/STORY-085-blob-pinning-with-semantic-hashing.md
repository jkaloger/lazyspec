---
title: Blob Pinning with Semantic Hashing
type: story
status: accepted
author: jkaloger
date: 2026-03-24
tags:
- certification
- refs
- blob-pinning
- semantic-hashing
related:
- implements: docs/rfcs/RFC-034-spec-certification-and-drift-detection.md
---




## Context

The `@ref` directive system links documents to code symbols, but currently has no mechanism for pinning references to a known baseline. Without pinning, there is no way to detect when referenced code has changed since it was last reviewed.

Commit-SHA-based pinning breaks under squash merge and GC. Blob hashes identify content rather than commits, surviving rebases, squash merges, and shallow clones. Symbol-level refs benefit from AST normalization (stripping comments, collapsing whitespace) so that formatting-only changes do not produce phantom drift. Whole-file refs hash raw content, since whitespace and comments may be meaningful in config files and schemas.

This story introduces the `@{blob:hash}` pinning suffix for `@ref` directives, the semantic hashing pipeline, and the `lazyspec pin` command that computes and writes blob hashes for all refs in a document.

## Acceptance Criteria

### AC: parse-symbol-blob-ref

Given a document containing `@ref path#symbol@{blob:hash}`
When the ref directive is parsed
Then the model stores the path, symbol, and blob hash as separate fields

### AC: parse-file-blob-ref

Given a document containing `@ref path@{blob:hash}`
When the ref directive is parsed
Then the model stores the path and blob hash, with no symbol field

### AC: unpinned-ref-unchanged

Given a document containing `@ref path#symbol` (no blob suffix)
When the ref directive is parsed
Then it is treated as an unpinned ref with a nil blob hash

### AC: symbol-semantic-hash

Given a source file with a named symbol
When the semantic hash is computed for that symbol
Then the AST is parsed with tree-sitter, comment nodes are stripped, whitespace is collapsed, and the normalized bytes are hashed via `git hash-object --stdin`

### AC: comment-change-no-drift

Given a pinned symbol-level ref whose blob hash was computed after normalization
When only comments or whitespace in the symbol body change (and no structural code changes occur)
Then recomputing the semantic hash produces the same blob hash

### AC: structural-change-drifts

Given a pinned symbol-level ref
When the symbol's code structure changes (e.g. new parameter, changed type, different logic)
Then recomputing the semantic hash produces a different blob hash

### AC: file-hash-raw-content

Given a whole-file ref (`@ref path@{blob:hash}`)
When the blob hash is computed
Then it uses `git hash-object` on the raw file content without normalization

### AC: pin-command-writes-hashes

Given a document with unpinned `@ref` directives
When `lazyspec pin <spec-id>` is run
Then each ref is resolved at HEAD, blob hashes are computed (normalized for symbol refs, raw for file refs), and `@{blob:hash}` suffixes are written into the directives

### AC: pin-command-updates-existing

Given a document with already-pinned `@ref` directives
When `lazyspec pin <spec-id>` is run
Then the existing `@{blob:hash}` suffixes are replaced with freshly computed hashes from HEAD

### AC: pin-unresolvable-ref-errors

Given a document with an `@ref` targeting a symbol that does not exist at HEAD
When `lazyspec pin <spec-id>` is run
Then the command reports an error for that ref and does not write a hash for it

### AC: normalize-config-default

Given no per-spec normalization override in `.lazyspec.toml`
When blob hashes are computed for symbol-level refs
Then AST normalization is applied (the default `normalize = true`)

### AC: normalize-config-opt-out

Given a `.lazyspec.toml` entry `[certification.overrides."<spec-path>"] normalize = false`
When blob hashes are computed for symbol-level refs in that spec
Then raw bytes are hashed without stripping comments or collapsing whitespace

### AC: git-hash-object-integration

Given normalized (or raw) bytes for a ref target
When the blob hash is computed
Then the bytes are passed to `git hash-object --stdin` and the resulting SHA is stored as the blob hash

## Scope

### In Scope

- Extending `@ref` directive syntax to support `@{blob:hash}` suffix (symbol-level and file-level)
- Parsing and storing the blob hash in the ref directive model
- AST normalization pipeline: tree-sitter parse, strip comment nodes, collapse whitespace
- Raw content hashing for whole-file refs (no normalization)
- `lazyspec pin <spec-id>` command to resolve refs at HEAD and write blob hashes
- Per-spec/per-language normalization opt-out in `.lazyspec.toml` (`[certification] normalize = true/false`)
- `git hash-object` integration for computing blob hashes from the working tree

### Out of Scope

- The `spec` document type, directory structure, or migration from `arch` (Story 1)
- Drift detection or signal collection using stored blob hashes (Story 3)
- Certification workflow, `lazyspec certify`, or frontmatter mutation (Story 4)
- The `affects` relationship type or coverage advisories (Story 5)
- Test execution integration
- Per-AC content hashing in `story.md` (Story 3/4 concern)
