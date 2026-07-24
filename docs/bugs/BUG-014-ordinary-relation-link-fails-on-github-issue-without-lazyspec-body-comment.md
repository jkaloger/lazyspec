---
title: "ordinary relation link fails on github issue without lazyspec body comment"
type: bug
status: reported
author: "jkaloger"
date: 2026-07-24
tags: []
related: []
---

## Context

Linking an ordinary (non-native) relation — e.g. `related-to` — on a github-issues doc whose remote issue body carries no lazyspec HTML comment fails hard:

```
$ lazyspec link PARENT-88 related-to PARENT-64
Error: no lazyspec HTML comment found in issue body
```

Issues created directly on GitHub (empty or prose-only body) hit this. Native relations (`implements` via sub-issue, `blocks` via dependency, `targets` via milestone) work on the same issues because they bypass the body merge entirely — which makes the failure look like "related-to doesn't show up" rather than "link failed".

## Root Cause

Ordinary relations round-trip through the issue body. `GithubIssuesStore::merge_relation_to_remote` (`src/engine/store_dispatch.rs`) parses the remote body with a hard `?`:

```rust
let (mut remote_meta, remote_prose) = issue_body::deserialize(&remote_issue.body, &ctx)?;
```

`issue_body::deserialize` → `extract_comment` (`src/engine/issue_body.rs:351`) errors when the body has no `<!-- lazyspec ... -->` comment. Link aborts before the remote write AND before the cache mirror (push-first design), so the edge never exists anywhere — nothing in the TUI Relations tab, nothing in `context`.

Causal chain:

1. Issue authored on GitHub → body `null` or plain prose, no lazyspec comment.
2. `lazyspec link A related-to B` → rel has no `github_native` → `merge_relation_to_remote`.
3. `deserialize` fails → whole link errors → no remote edit, no cache frontmatter write.
4. `unlink` dies on the same path. TUI link editor (`confirm_link`) surfaces the same error in `link_editor.error`.

The fetch side already tolerates this: `parse_issue` (`src/engine/issue_cache.rs`) falls back to synthesizing meta from issue fields when `deserialize` fails. The write-merge side has no such fallback — asymmetric.

Distinct from BUG-013 (traversal-marker gating in `resolve_chain`): here the relation is never persisted at all.

## Expected vs Actual

- **Expected:** linking an ordinary relation on any github-issues doc succeeds; the first lazyspec write adopts a GitHub-authored issue by adding the lazyspec comment, preserving the existing body as prose.
- **Actual:** link/unlink error out on comment-less bodies; the relation is silently absent from every surface.

## Repro

1. Create an issue directly on GitHub (no lazyspec comment in body) under a github-issues type; `lazyspec fetch`.
2. `lazyspec link <that-doc> related-to <any-doc>` → `Error: no lazyspec HTML comment found in issue body`.
3. Relations tab / `lazyspec context` show nothing (edge never written).

## Fix Direction

In `merge_relation_to_remote`, fall back on deserialize failure: synthesize meta from remote issue fields (title/labels/state → type/tags/status, empty `related`) mirroring `parse_issue`'s fallback, treat the entire remote body as prose, then merge the edge and serialize. First ordinary link adopts the issue. Consider extracting the fallback shared with `parse_issue`. Check `push_cache`/`check_lock` and the clickup merge path for the same class of failure.

## Acceptance Criteria

- [ ] `link`/`unlink` of an ordinary relation succeeds on a github-issues doc whose remote body has no lazyspec comment.
- [ ] The pre-existing remote body text is preserved as prose beneath the newly written comment.
- [ ] Native-relation behaviour unchanged.
- [ ] Test covers link + unlink against a comment-less remote body.
- [ ] Full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.

