---
title: derive author from GitHub issue creator
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- implements: STORY-101
---




## Context

The github-issues store sets `author` from embedded YAML in the issue body, falling back to `"unknown"`. It never looks at the GitHub issue's actual creator. Documents created by `@jkaloger` show up as `author: unknown` or whatever was passed at creation time, instead of `@jkaloger`.

Fix: derive `author` from the `gh` CLI's `author.login` field, which is already available in the same `--json` request used for other fields.

## Changes

### Task 1: Add `author` field to `GhIssue`

ACs addressed: GH issue author is available in parsed issue data

Files:
- Modify: `src/engine/gh.rs`

What to implement:
- Add `GhAuthor` struct: `{ login: String }` with `Deserialize, Debug, Clone, PartialEq, Eq`
- Add `author: Option<GhAuthor>` field to `GhIssue` (with `#[serde(default)]`)
- Add `"author"` to the default field string in `issue_list` (line 206): `"number,url,title,body,labels,state,updatedAt,author"`
- Add `"author"` to the `--json` field string in `issue_view` (line 239): `"number,url,title,body,labels,state,updatedAt,author"`
- Update `MockGhClient` default issue builders to include `author: None` or a test value

How to verify:
- Existing tests in `gh.rs` continue to pass (partial JSON deserialization tolerates missing `author`)
- `parse_issue_json` with `"author":{"login":"jkaloger"}` returns `Some(GhAuthor { login: "jkaloger" })`

### Task 2: Use `GhIssue.author` in `parse_issue` and remove `author` from issue body YAML

ACs addressed: cached documents get author from GH issue creator, not embedded YAML

Files:
- Modify: `src/engine/issue_cache.rs` (fn `parse_issue`)
- Modify: `src/engine/issue_body.rs` (fn `serialize`, fn `deserialize`, `CommentFrontmatter`)

What to implement:

In `issue_cache.rs`:
- Change `parse_issue` signature to accept `&GhIssue` fields that include `author`
- After calling `issue_body::deserialize`, override `meta.author` with `issue.author.as_ref().map(|a| format!("@{}", a.login)).unwrap_or_else(|| "unknown".to_string())`
- In the fallback path (no lazyspec comment), same logic: use `issue.author` instead of hardcoded `"unknown"`

In `issue_body.rs`:
- Remove `author` from `serialize()` output (drop the `author: {}` YAML line)
- Remove `author` from `CommentFrontmatter` struct (make it tolerate missing `author` for backward compat with existing issues that still have it)
- In `deserialize()`, return a placeholder author (e.g. `"unknown"`) since the caller (`parse_issue`) will override it with the GH author anyway
- Keep backward compat: `CommentFrontmatter` should use `#[serde(default)]` for `author` so existing issues with embedded author don't fail to parse

How to verify:
- `cargo test` passes
- Round-trip tests in `issue_body.rs` updated to reflect no `author` in serialized output
- `parse_issue` tests verify author comes from `GhIssue.author.login`

### Task 3: Update `store_dispatch` create path

ACs addressed: newly created issues also use GH author consistently

Files:
- Modify: `src/engine/store_dispatch.rs`

What to implement:
- In `GithubIssuesStore::create`, the `author` param is still passed in and stored in `DocMeta` for the cache file. This is fine since `issue_body::serialize` will no longer embed it. The cache file frontmatter (`write_cache_file`) will still have the author from the create call.
- In `GithubIssuesStore::update`, when re-serializing after edits, `issue_body::serialize` will no longer include author. No code change needed beyond what Task 2 handles, but verify the update path still works.

How to verify:
- Integration test: create a document, verify the issue body does not contain `author:` in the lazyspec comment
- Update path: edit a document, verify the re-serialized body has no `author:` line

## Test Plan

1. **`gh.rs`: `GhAuthor` deserialization** - Parse JSON with `"author":{"login":"jkaloger"}` and verify the field. Parse JSON without `author` field and verify `None`. Isolated, fast, deterministic.

2. **`issue_body.rs`: serialize omits author** - Call `serialize()` and assert output does not contain `author:`. Update existing round-trip tests. Isolated, fast, behavioral.

3. **`issue_body.rs`: deserialize tolerates missing author** - Parse a body with no `author` in the YAML block. Parse a body with `author` still present (backward compat). Both should succeed. Isolated, fast, structure-insensitive.

4. **`issue_cache.rs`: `parse_issue` uses GH author** - Construct a `GhIssue` with `author: Some(GhAuthor { login: "jkaloger" })`, call `parse_issue`, assert `meta.author == "@jkaloger"`. Also test with `author: None` and assert fallback to `"unknown"`. Isolated, fast, behavioral.

5. **`issue_cache.rs`: `fetch_all` end-to-end** - Mock reader returns issues with `author` field. After fetch, read cache files and verify `author: "@jkaloger"` in frontmatter. Tradeoff: slightly less isolated (touches filesystem), but predictive of real behavior.

## Notes

The `gh issue list --json author` returns `{"author":{"login":"username"}}`. Using `Option<GhAuthor>` keeps backward compat with tests that construct `GhIssue` without the field.

Prefixing with `@` (e.g. `@jkaloger`) matches GitHub username convention and distinguishes GH-sourced authors from freeform names used in filesystem documents.
