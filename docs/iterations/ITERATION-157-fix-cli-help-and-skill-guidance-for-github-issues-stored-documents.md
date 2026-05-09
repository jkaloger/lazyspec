---
title: "Fix CLI help and skill guidance for github-issues stored documents"
type: iteration
status: accepted
author: "agent"
date: 2026-04-09
tags: []
related: []
---


## Context

Agents working with github-issues stored documents misuse cache paths as if they were filesystem documents. Observed: an agent created a story, then linked it using the `.lazyspec/cache/` path, then edited the cached file directly. The cache is a read-only mirror of GitHub, not the source of truth.

Root causes:
1. CLI help text for `link`, `update`, `unlink`, and `delete` says "Document path" but these commands also accept shorthand IDs (e.g. STORY-095). The `show` command already documents this correctly. Agents reading `--help` conclude they need a file path, and the only path available for github-issues docs is the cache path.
2. `create` has no `--body`/`--body-file` flags, so agents create a document then look for a file to edit, finding the cache file.
3. Skills say "don't write document files directly" but don't explain what to do instead for github-issues documents.

## Changes

1. **Update CLI help text for `link`, `update`, `unlink`, `delete` to match `show`**
   - File: `src/cli.rs`
   - Lines 93, 114, 120, 126, 132, 139
   - Change doc comments from `/// Document path` / `/// Source document path` / `/// Target document path` to include "or shorthand ID (e.g. RFC-001)" matching the `show` command pattern on line 78
   - Verification: `cargo run -- help link`, `cargo run -- help update`, `cargo run -- help unlink`, `cargo run -- help delete` all show "path or shorthand ID" in argument descriptions

2. **Add `--body` and `--body-file` flags to `create` command**
   - File: `src/cli.rs` lines 50-63 (Create variant)
   - Add `body: Option<String>` and `body_file: Option<String>` matching the existing pattern in `Update` (lines 102-107)
   - File: `src/main.rs` lines 84-103 (Create handler)
   - After `create::run` / `create::run_json`, if body/body_file provided, call `update` logic on the newly created document to set the body
   - File: `src/cli/create.rs` (handler implementation)
   - Verification: `cargo run -- help create` shows `--body` and `--body-file` flags; `cargo run -- create iteration "test" --author agent --body "content here" --json` creates a document with body content

3. **Add github-issues guidance to skill NEVER/preamble blocks**
   - Files: `.claude/skills/{plan-work,create-story,create-iteration,build,write-rfc,review-iteration,resolve-context,create-audit}/SKILL.md`
   - After the existing `<NEVER>` block in each skill, add a `<GITHUB-ISSUES-DOCUMENTS>` section with three rules:
     - Never edit files under `.lazyspec/cache/`. These are read-only mirrors. Use `lazyspec update <ID>` to modify document content.
     - Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
     - To set body content: use `--body` or `--body-file` on `lazyspec create`, or `lazyspec update <ID> --body` after creation.
   - Verification: read each SKILL.md, confirm the section is present and consistent

## Test Plan

- **CLI help text (behavioral, automated):** Integration test that invokes the CLI help for `link`, `update`, `unlink`, `delete`, `create` and asserts the argument descriptions contain "shorthand ID". Test file: `tests/cli_help.rs` or extend existing CLI tests.
- **Create with --body (behavioral, automated):** Integration test that runs `create iteration "test" --body "some content"` in a temp project, then runs `show` on the created doc and asserts the body contains "some content". Extend existing create tests.
- **Create with --body-file (behavioral, automated):** Same as above but writes content to a temp file and passes `--body-file <path>`.
- **Skill guidance (manual):** Read each SKILL.md and verify the github-issues section is present.

## Notes

The `create` command currently delegates to `create::run` / `create::run_json` which return the created document's path/metadata. The body application can be a sequential step: create the document, then call the same update logic that `update` uses. This avoids duplicating body-writing logic in the create path.

The skill guidance is a separate `<GITHUB-ISSUES-DOCUMENTS>` block rather than folded into the existing `<NEVER>` block, so it's visually distinct. The same text goes in all skills for consistency.
