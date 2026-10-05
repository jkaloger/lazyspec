---
title: Deleting a bundle removes only index.md
type: bug
status: reported
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- related-to: STORY-291
- related-to: RFC-074
---

## Summary

`lazyspec delete <id>` on a document bundle removes only `index.md`. Parts and the bundle directory remain on disk.

## Reproduction

1. Create a bundle with at least one part (STORY-291).
2. `lazyspec delete <id>`.
3. Bundle directory still holds the parts; the document is gone from the index but its files are orphaned.

## Expected

Delete removes the whole bundle directory.

## Actual

`delete_document` (`src/engine/fs_ops.rs:395-407`) calls `fs::remove_file` on the resolved doc path. For a bundle that path is `index.md`.

## Fix direction

When the resolved doc is a bundle, remove the named files, then the directory if empty. Never `remove_dir_all`.

For an `index.md` doc: remove the index plus its `parts` and `sidecars` (a missing one counts as already gone), then `fs::remove_dir` the folder, ignoring "directory not empty". Children of a shared folder survive. Keep the guard that the folder is not the project root.

Why not recursive: `docs/rfcs/index.md` can be a plain doc in the type directory, and a recursive delete would wipe every sibling. A folder can also hold child `.md` files that failed to parse; they are not in the store's children, so a recursive delete destroys them.

Other stores:

- `git` store: delete goes through the filesystem fix in the clone, then commits with `add -A`, which stages every removal. Test added in `git_store.rs`.
- Extends-backed path: covered by the same fix through `commit_if_extends_backed` (`add -A`).
- `git-ref` store: not affected. One blob per doc, no bundles.

## Related

Bundles: RFC-074, STORY-291.
