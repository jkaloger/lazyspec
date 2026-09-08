---
title: Stamp reviewed when I move a document's status
type: story
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
---

As a document author who has just moved a document's status after checking it against the code, I want that transition to record the current commit in `reviewed`, so that the review I just did resets the staleness clock without a second command.

## Acceptance criteria

- Given a document at any status, when I run `update <id> --status <next>`, then `reviewed` is written with the current `HEAD` sha and the transition succeeds.
- Given a status change made from the TUI, the web status form, or the advance skill, when it completes, then `reviewed` is stamped the same way, because all of them route through `Store::update_status`.
- Given a `drift` type document reported `stale`, when I transition its status and re-run `validate`, then it no longer reports `stale`.
- Given a type with `status_authority` set and a status arriving through `fetch`, when the store is written, then `reviewed` is left unchanged.
- Given that same type, when a human runs `update <id> --status <next>` locally, then the board card moves and `reviewed` is stamped.
- Given a repository with no commits or a detached state where `HEAD` cannot be read, when I run `update <id> --status <next>`, then the transition still succeeds and `reviewed` is left unchanged.

## Notes

One engine hook, `Store::update_status`, covers the CLI, TUI, web and skill callers. A local transition is a human looking at the document against current code; a status synced from a project board is not. `pin <id>` (STORY-270) remains the way to stamp without a transition.
