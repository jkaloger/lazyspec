---
title: 'Batch cleanup: RefList naming, unwrap in fix, bare-query glob match'
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
---

Small items found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`), none of them blocking. Grouped because each is a few lines.

1. **`RefList` now types two different things.** `src/engine/git_ref.rs:382` is reused for rename pairs, though elsewhere it means `(ref, sha)`. Rename it for what it is, or inline the pair type at each use.

2. **`unwrap()` outside tests**, against DICTUM-001. `src/cli/fix.rs:156` in `governs_json`. It matches the existing pattern in `run_config` and `run`, so fix all three or leave all three — a lone correction just makes the file inconsistent.

3. **A bare relative query is matched against every glob.** When `governs_root == root`, any query strips cleanly to a relative path, so every keystroke runs the glob matchers against the raw query string. Harmless at present, but a document pinning `**` or `*.md` would match every search. Consider requiring the query to look like a path before treating it as one.
