---
title: TUI link edits propagate to GitHub Issues frontmatter
type: iteration
status: accepted
author: jkaloger
date: 2026-03-28
tags: []
related:
- implements: STORY-098
---




## Context

`cli::link::link()` writes frontmatter to the local cache file but never pushes changes back to GitHub. Status updates work because they go through `cli::update::run_with_config()` → `gh_store.update()`. Link edits bypass this entirely.

## Changes

1. In `cli::link::link()` and `cli::link::unlink()`: after writing the cache file, detect if the document is GitHub-backed (check if path is under `.lazyspec/cache/`). If GitHub-backed, read the updated cache file and push the full body (with updated frontmatter) to GitHub via `gh_store.update()` with a "body" field. Follow the pattern from `cli::update.rs` which already does this for status changes. Extend `store_dispatch.rs` `GithubIssuesStore::update()` if needed. Add a unit test that verifies `link()` on a GitHub-backed document triggers a push to GitHub.
2. In `tui/state/app.rs` `confirm_link()`: ensure the GitHub push path is triggered after `link()` completes, matching how status updates already call through to the GitHub store. Verify with existing TUI test patterns.

## Test Plan

- Unit test: `link()` on a GitHub-backed document triggers a push to GitHub (mock the API call)
- Manual: add a relationship in the TUI to a GH-issues doc, verify the GitHub issue body updates with the new `related:` entry
- Manual: remove a relationship, verify it's removed from the GitHub issue body

## Notes

ACs addressed: TUI link/unlink edits sync to GitHub Issues, matching how status updates already work

Files:
- Modified: `src/cli/link.rs`, `src/tui/state/app.rs`, possibly `src/engine/store_dispatch.rs`
- Comparison: `src/cli/update.rs` (working pattern for GitHub push)
