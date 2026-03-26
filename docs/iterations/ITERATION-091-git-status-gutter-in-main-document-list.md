---
title: Git status gutter in main document list
type: iteration
status: accepted
author: agent
date: 2026-03-23
tags: []
related:
- implements: STORY-077
---




## Changes

### Task 1: Git status query and cache module

ACs addressed: AC-5 (single git status call on first render, cached), AC-6 (cache invalidation on create/save/delete), AC-7 (no errors outside git repo)

Files:
- Create: `src/engine/git_status.rs`
- Modify: `src/engine/mod.rs` (add `pub mod git_status;`)

What to implement:

Define a `GitFileStatus` enum with variants `New`, `Modified`. Define a `GitStatusCache` struct holding a `HashMap<PathBuf, GitFileStatus>` and a `stale: bool` flag.

Add a `query_git_status(repo_root: &Path) -> Option<HashMap<PathBuf, GitFileStatus>>` function that:
- Runs `git status --porcelain` via `std::process::Command` in the given directory
- Returns `None` if git is not available or the directory is not a repo (non-zero exit / missing `.git`)
- Parses each line: first two characters are the status code, remainder (after space) is the file path
  - `??` or `A ` or `AM` → `New`
  - `M `, ` M`, `MM` → `Modified`
  - `R `, `RM` → `Modified` (renamed; use the destination path after ` -> `)
  - All other codes with changes → `Modified`
- Returns `Some(map)` with paths resolved relative to `repo_root`

Add methods on `GitStatusCache`:
- `new(repo_root: &Path) -> Self` — calls `query_git_status`, stores result
- `invalidate(&mut self)` — sets `stale = true`
- `refresh(&mut self, repo_root: &Path)` — re-queries if stale
- `get(&self, path: &Path) -> Option<&GitFileStatus>` — lookup

How to verify: `cargo test -- git_status` runs unit tests from Task 4.

---

### Task 2: Integrate git status cache into App state

ACs addressed: AC-5 (cache populated on startup), AC-6 (invalidation on events), AC-7 (graceful outside git repo)

Files:
- Modify: `src/tui/app.rs` — add `git_status_cache: GitStatusCache` field to `App`, populate in `App::new`, call `invalidate()` in `build_doc_tree`
- Modify: `src/tui/mod.rs` — call `git_status_cache.refresh()` after `CreateComplete` and `FileChange` events that modify `.md` files

What to implement:

Add `git_status_cache: GitStatusCache` to the `App` struct. In `App::new`, construct it with `GitStatusCache::new(&store.root)`. The `store.root` (or equivalent path to the repo root) is passed to the cache.

In `handle_app_event` (`mod.rs`):
- After `AppEvent::CreateComplete` (success path): call `app.git_status_cache.invalidate()`
- After `AppEvent::FileChange` for `.md` files: call `app.git_status_cache.invalidate()`

In `draw_doc_list` or at the start of `ui::draw`: call `app.git_status_cache.refresh(&root)` so stale caches are refreshed before rendering.

How to verify: `cargo test -- tui` runs integration tests from Task 4.

---

### Task 3: Render gutter column in document list

ACs addressed: AC-1 (green for new/untracked), AC-2 (yellow for modified), AC-3 (empty for unchanged), AC-4 (partially staged as modified), AC-8 (renamed as modified)

Files:
- Modify: `src/tui/ui.rs` — `doc_table_widths`, `doc_row_for_node`, `draw_doc_list`

What to implement:

In `doc_table_widths`: change return type to `[Constraint; 6]`. Insert `Constraint::Length(1)` as the first element, shifting existing columns right.

In `doc_row_for_node`: look up `node.path` in `app.git_status_cache.get()`. Create a gutter `Cell`:
- `Some(GitFileStatus::New)` → `Cell::from("┃").style(Style::default().fg(Color::Green))`
- `Some(GitFileStatus::Modified)` → `Cell::from("┃").style(Style::default().fg(Color::Yellow))`
- `None` → `Cell::from(" ")`

Prepend this cell before the existing tree cell. The row is now 6 cells wide.

Update the second usage of `doc_table_widths` (line ~1533, search overlay) to match the new 6-column layout. If the search overlay calls `doc_row_for_node` too, the gutter will render there automatically.

How to verify: `cargo run` — open the TUI in a git repo with modified files and visually confirm colored bars. Automated tests in Task 4.

---

### Task 4: Tests

ACs addressed: all 8 ACs

Files:
- Create: `tests/tui_git_status.rs`
- Modify: `tests/common/mod.rs` (if helper additions needed)

What to implement:

Use `TestFixture::with_git_remote()` to create a fixture with a git repo. The tests exercise the `GitStatusCache` and the gutter rendering through `App` state.

Planned tests:

1. `test_new_file_shows_green` — create fixture with git repo, add a new doc file (don't commit), build App, assert `git_status_cache.get(path)` returns `Some(New)`
2. `test_modified_file_shows_yellow` — commit a doc, modify it, build App, assert `get(path)` returns `Some(Modified)`
3. `test_unchanged_file_no_indicator` — commit a doc, don't modify, assert `get(path)` is `None`
4. `test_cache_invalidation` — build App, modify a file, call `invalidate()` + `refresh()`, assert new status is reflected
5. `test_non_git_repo` — use `TestFixture::new()` (no git init), build App, assert cache returns `None` for all paths and no panic
6. `test_porcelain_parsing` — unit test `query_git_status` with various porcelain output scenarios (renamed, partially staged, untracked)

Tradeoffs: tests 1-5 invoke real git commands in a temp dir. This sacrifices speed for predictiveness — real git behavior is tested, not mocked parsing. Test 6 is a faster unit test for parsing edge cases. The `with_git_remote` fixture already handles setup/teardown.

How to verify: `cargo test -- git_status tui_git_status`

## Test Plan

| AC | Test | What it verifies |
|----|------|-----------------|
| AC-1 (green for new) | `test_new_file_shows_green` | Untracked file → `New` status in cache |
| AC-2 (yellow for modified) | `test_modified_file_shows_yellow` | Modified committed file → `Modified` in cache |
| AC-3 (empty for unchanged) | `test_unchanged_file_no_indicator` | Committed, unmodified file → `None` in cache |
| AC-4 (partially staged) | `test_porcelain_parsing` | `MM` status code → `Modified` |
| AC-5 (single git status call) | `test_new_file_shows_green` | Cache is populated after `App::new` |
| AC-6 (invalidation) | `test_cache_invalidation` | After invalidate + refresh, new state is returned |
| AC-7 (outside git repo) | `test_non_git_repo` | No panic, no indicators, cache returns `None` |
| AC-8 (renamed file) | `test_porcelain_parsing` | `R ` status code → `Modified` with destination path |

## Notes

- Consistent with existing codebase: git interaction via `std::process::Command`, not `git2` crate
- The `doc_table_widths` change from 5 to 6 columns affects all views using that function, which is desirable for Story 2 (search/filtered views) later
- ADR-005 documents the caching strategy decision
