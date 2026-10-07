---
title: "Editing a bundle part raises a false push conflict"
type: bug
status: reported
author: Jack Kaloger
date: 2026-10-07
tags: []
related:
- related-to: STORY-294
---

## Summary

TUI `e` on a `§` part row, save, exit: Conflict overlay "document not found in store". Edit saved locally; the push threads fail.

## Reproduction

1. Bundle doc with parts (e.g. `DELTA-001/{index,design}.md`), filesystem store.
2. TUI, select `§ design`, `e`, save, quit editor.
3. Conflict overlay shows.

## Expected

No overlay. Part edits route through the bundle root's store/type.

## Actual

`event_loop.rs` spawns every backend push after any edit. Each looks up `store.get(relative)` with the part path. Parts are not in `store.docs`, only on the index's `parts`, so:

- `try_push_git_ref_edit` -> `Err("document not found in store")`.
- `try_push_clickup_edit` / `try_push_gh_edit` parse frontmatter before the type check -> `Err("no frontmatter found")`.

## Fix direction

Resolve `relative` via `store.bundle_root` before lookup; check the store backend before parsing. git-ref re-commits the whole doc by id, so part edits push. github-issues/clickup push one body; a part's body would clobber the remote, so part edits there no-op until body stitching exists.

Rejected: skip all pushes for parts in the event loop. Drops the git-ref re-commit.
