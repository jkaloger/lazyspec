---
title: Staleness cache written into working tree without gitignore
type: bug
status: reported
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- related-to: STORY-276
---

## Summary

`context` and `show` write `.lazyspec/cache/` into the working tree without gitignoring it. Users see untracked cache files in `git status`.

## Reproduction

1. Fresh repo, no `.lazyspec/.gitignore` entry for `cache/`.
2. `lazyspec context <id>` or `lazyspec show <id>`.
3. `git status` lists `.lazyspec/cache/`.

## Expected

`cache/` is listed in `.lazyspec/.gitignore` before the first cache write.

## Actual

`src/engine/staleness_cache.rs:68` writes `.lazyspec/cache/<CACHE_FILE>` and never calls `crate::engine::store::ensure_cache_gitignored` (`src/engine/store.rs:191`). `init`, `extends`, and `git-ref` already call it.

## Fix direction

Call `ensure_cache_gitignored` before the first write in `staleness_cache.rs`.

Rejected: writing the cache under `.git/`. Breaks in worktrees and in non-git roots.

## Related

Staleness cost work: STORY-276.
