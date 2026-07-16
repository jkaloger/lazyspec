---
title: "Opening $EDITOR from the TUI stalls ~30s on Linux"
type: bug
status: triaged
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Summary

Pressing `e` in the TUI can take ~30 seconds before $EDITOR appears, observed on Linux.

## Reproduction

1. Repo with a remote-backed store (github-issues and/or git-ref types) and a slow or auth-prompting remote.
2. Open the TUI; within the first minute (or any 60s poll window), press `e` on a doc.
3. Editor appears only after tens of seconds.

## Expected

Editor opens near-instantly; background sync never blocks the UI thread.

## Actual

Main loop stalls before spawning the editor.

## Root cause

The poll-sync thread holds the shared `GithubIssuesStore` mutex for the entire network sync (`src/tui/infra/event_loop.rs:304`, comment at 301-303), which includes `gh` CLI fetches and `git fetch --prune origin` with no timeout (`src/engine/sync.rs:391` → `src/engine/git_ref.rs:216-221`). The first poll fires immediately at startup (`event_loop.rs:655-656`) and repeats every `cache_ttl` (default 60s). On the same loop iteration that handles the `e` keypress, the `gh_issue_map_stale` branch does a blocking `shared_store.lock()` (`event_loop.rs:751`) *before* the editor block runs (`event_loop.rs:824-830`) — so the UI thread waits out the whole remote sync, then opens the editor. The editor spawn itself (`run_editor`, `event_loop.rs:49-65`) is clean.

Secondary defect, same path: `Command::new(&editor)` treats the whole $EDITOR string as one binary — `EDITOR=\"code --wait\"` fails to spawn (prints error, no hang).

## Fix direction

Never hold the store mutex across network I/O on a path the UI thread also locks: sync into a snapshot and swap under a short lock, or make the UI-thread lock attempts non-blocking (`try_lock` + retry next tick). Add timeouts to `git fetch`/`gh` subprocess calls. Split $EDITOR on whitespace for args.
