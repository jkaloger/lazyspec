---
title: Link ordinary relations on GitHub-authored issues
type: story
status: draft
author: jkaloger
date: 2026-07-24
tags: []
related:
- related-to: BUG-014
---

## Problem

As a user linking docs, I can't add an ordinary relation (`related-to`, `supersedes`, …) to a github-issues doc whose issue was authored directly on GitHub. The remote body has no lazyspec HTML comment, and the body-merge write path (`GithubIssuesStore::merge_relation_to_remote`) hard-fails on `issue_body::deserialize`:

```
$ lazyspec link PARENT-88 related-to PARENT-64
Error: no lazyspec HTML comment found in issue body
```

The link aborts before the remote write and before the cache mirror, so the relation exists nowhere — Relations tab and `context` show nothing. Native relations (sub-issue, dependency, milestone) work on the same issues because they bypass the body, which disguises the failure as "related-to doesn't show up". The fetch side already tolerates comment-less bodies (`parse_issue` falls back to synthesized meta); the write side doesn't. Root cause in BUG-014.

## Goal

Linking or unlinking an ordinary relation on any github-issues doc succeeds regardless of whether the remote body carries a lazyspec comment. The first lazyspec write **adopts** a GitHub-authored issue: it writes the lazyspec comment with the merged relation and preserves the entire pre-existing body as prose beneath it. Behaviour is identical from CLI `link`/`unlink` and the TUI link editor (both share `ops::link`).

## Design

In `merge_relation_to_remote` (src/engine/store_dispatch.rs), when `issue_body::deserialize` fails, fall back the way `parse_issue` (src/engine/issue_cache.rs) does: synthesize `DocMeta` from the remote issue's fields (title, labels → type/tags, open/closed → lifecycle status, created date, empty `related`), and treat the whole remote body as prose. Then apply the relation delta and serialize as normal. Extract the fallback shared with `parse_issue` only if it falls out naturally — two call sites justify it (convention principle 6).

Out of scope: the clickup merge path and `push_cache`/`check_lock` — audit them for the same failure class and file follow-ups if affected, but don't bundle fixes here.

## Acceptance Criteria

- [ ] Given a github-issues doc whose remote body is empty or prose-only (no lazyspec comment), when I `lazyspec link <doc> related-to <other>`, then the link succeeds, the remote body gains a lazyspec comment carrying the relation, and the relation appears in `context --json` and the TUI Relations tab.
- [ ] Given that adopted issue, when I `lazyspec unlink` the same relation, then the unlink succeeds and the relation is removed from the remote comment and the cache.
- [ ] Given a remote body with pre-existing prose and no comment, when the first ordinary link lands, then the prose is preserved verbatim beneath the new comment.
- [ ] Native-relation link/unlink behaviour is unchanged.
- [ ] Full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.

