---
title: TUI async ref expansion and skill updates
type: iteration
status: accepted
author: jkaloger
date: 2026-03-12
tags: []
related:
- implements: docs/stories/STORY-058-ref-expansion-hardening-and-performance.md
---



## Changes

### Task Breakdown

1. **Add expanded body cache and expansion state to App**
   - Add fields to the App struct (likely in `src/tui/app.rs` or `src/tui/mod.rs`):
     - `expanded_body_cache: HashMap<PathBuf, String>` -- keyed by doc path
     - `expansion_in_flight: Option<PathBuf>` -- which doc is currently expanding
   - Add an `mpsc::Receiver<(PathBuf, String)>` field for receiving expansion results from background threads.
   - Store the corresponding `mpsc::Sender` in a field or pass it to the spawn function.
   - File: TUI app state file

2. **Spawn background expansion thread on document select**
   - When the selected document changes (detect in the main TUI loop or key handler):
     - If the new doc's path is already in `expanded_body_cache`, do nothing.
     - Otherwise, set `expansion_in_flight = Some(doc.path.clone())`.
     - Clone the `Store` (or just the `RefExpander` + file path) and the sender.
     - `std::thread::spawn` a closure that calls `get_body_expanded()` and sends `(path, expanded_body)` through the channel.
   - File: `src/tui/mod.rs`

3. **Poll expansion results in the main loop**
   - In the main TUI event loop (after `terminal.draw` and before/after key event handling), call `receiver.try_recv()`.
   - On `Ok((path, body))`: insert into `expanded_body_cache`, clear `expansion_in_flight` if it matches.
   - On `Err(TryRecvError::Empty)`: do nothing (still loading).
   - File: `src/tui/mod.rs`

4. **Update `draw_preview_content` and `draw_fullscreen` to use cache**
   - In `draw_preview_content` (`src/tui/ui.rs:385`): check `app.expanded_body_cache.get(&doc.path)`. If present, use it. If not, call `get_body_raw()` (instant, no git).
   - Show a `[expanding refs...]` line at the top of the body when `app.expansion_in_flight == Some(doc.path)`.
   - Same logic for `draw_fullscreen` (`src/tui/ui.rs:627`).
   - File: `src/tui/ui.rs`

5. **Invalidate cache on document switch and file watch**
   - When selected document changes: only start a new expansion, don't clear the whole cache (previous expansions are still valid).
   - When the file watcher fires a modify event for a doc path: remove that path from `expanded_body_cache` so it re-expands on next view.
   - When the file watcher fires for a non-doc file (could be a referenced source file): clear entire cache (conservative approach, since we don't track which refs point where).
   - File: `src/tui/mod.rs`

6. **Update `./skills/build/SKILL.md` with @ref guidance**
   - In the implementer prompt section, add a note about `@ref` directives:
     - Syntax: `@ref <path>[#symbol][@sha]`
     - Use in docs to reference live code types
     - CLI: `lazyspec show -e <id>` to see expanded output
   - File: `skills/build/SKILL.md`

7. **Update `./skills/write-rfc/SKILL.md` with expanded @ref docs**
   - Expand the existing `@ref` mention (around line 88-91) to include:
     - Full syntax with `@sha` for pinned references
     - Note that `lazyspec show -e` is needed to preview expansion
     - When to use `@ref` vs `@draft`
   - File: `skills/write-rfc/SKILL.md`

8. **Update `./skills/resolve-context/SKILL.md` with -e flag guidance**
   - Add to the "Read document bodies" step: mention that `lazyspec show -e <id>` expands `@ref` directives and is useful when the agent needs to see the actual type definitions referenced in a doc.
   - File: `skills/resolve-context/SKILL.md`

## Test Plan

| AC | Test | Validates |
|----|------|-----------|
| TUI shows raw then expanded | Manual test: open TUI, select doc with @ref, observe loading -> expanded transition | Async expansion works |
| Cache invalidation | Manual test: edit a referenced source file, observe TUI preview updates | File watch triggers re-expansion |
| Document switch discards stale | Manual test: select doc A (slow @ref), quickly switch to doc B, verify B loads without waiting for A | In-flight handling |
| Skills updated | Read skill files, verify @ref and -e flag are documented | Agent guidance |
| Full suite | `cargo test` passes | Nothing broken |

> [!NOTE]
> TUI async behavior is difficult to unit test. The test plan relies on manual verification for the async paths. The synchronous expansion logic is already covered by ITERATION-053 and ITERATION-054 tests.

## Notes

- Depends on ITERATION-053 (RefExpander) and ITERATION-054 (get_body_raw/get_body_expanded split).
- `Store` is not `Send` by default (HashMap internals are fine, but worth verifying). If cloning Store for the thread is too expensive, clone just the RefExpander + the file content.
- The `mpsc` channel approach avoids pulling in an async runtime (tokio). The TUI already has a 100ms poll timeout which is a natural place to check the channel.
- Conservative cache invalidation (clear all on non-doc file change) is simpler than tracking ref dependencies. Can optimize later if it causes UX issues.
