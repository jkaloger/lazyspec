---
title: Git branch, search, and type filter status bar components
type: iteration
status: accepted
author: agent
date: 2026-03-30
tags: []
related:
- implements: STORY-106
---



## Changes

### Task 1: Add git branch detection and store on App

ACs addressed: AC-1 (branch displayed in git repo), AC-2 (no branch outside git repo), AC-3 (no error when git unavailable)

Files:
- Modify: `src/engine/git_status.rs`
- Modify: `src/tui/state/app.rs`

What to implement:

Add a `query_git_branch(repo_root: &Path) -> Option<String>` function to `git_status.rs`. It runs `git rev-parse --abbrev-ref HEAD` with `current_dir(repo_root)`. Returns `None` if the command fails (not a repo, git not installed, any error). Uses `Command::new("git")` consistent with the existing `query_git_status` pattern.

Add a `pub git_branch: Option<String>` field to `App`. In `App::new()`, call `query_git_branch` with the store's root path and store the result. The branch is read once at startup and never refreshed (per RFC-022 design).

How to verify:
- `cargo test` passes
- Unit test in `git_status.rs`: `query_git_branch` returns `Some` when run inside a git repo
- Unit test: `query_git_branch` returns `None` for a temp directory that is not a git repo

---

### Task 2: Implement git_branch, search, and type_filter components

ACs addressed: AC-1 (branch in status bar), AC-4 (search query displayed), AC-5 (search query removed on exit), AC-6 (type filter displayed), AC-7 (type filter removed when cleared)

Files:
- Modify: `src/tui/views/status_bar.rs`

What to implement:

Add three component functions following the existing `fn(&App) -> Option<Span<'static>>` pattern:

`git_branch_component`: returns `app.git_branch.as_ref().map(|b| Span::raw(format!(" {}", b)))` with `Color::Cyan` foreground. Returns `None` when `git_branch` is `None`. The `` is a git branch icon (U+E0A0, Powerline glyph), but use a plain ` ` prefix if that causes issues in standard terminals.

`search_component`: if `app.search_mode` is true and `app.search_query` is not empty, returns `Some(Span::raw(format!("/{}", app.search_query)))` styled with `Color::Yellow` foreground. Returns `None` otherwise. The `/` prefix mirrors vim search convention.

`type_filter_component`: when `app.view_mode` is `ViewMode::Types`, returns `Some(Span::raw(app.current_type().name.clone()))`. Returns `None` for other view modes, since the type filter only applies in Types mode.

Update `StatusBarComponents::default()`:
- left: `[mode_component, type_filter_component, doc_count_component]`
- right: `[git_branch_component, search_component, version_component, help_hint_component]`

How to verify:
- `cargo test` passes
- Unit tests per component (see Test Plan)

## Test Plan

Tests in `tests/tui_status_bar_test.rs`, extending the existing integration test file:

`test_git_branch_component_returns_none_when_no_branch`: create an App from a TestFixture (default `git_branch` is `None`), assert `git_branch_component(&app).is_none()`. Tests AC-2 and AC-3. Isolated, fast, deterministic.

`test_git_branch_component_returns_span_when_branch_set`: create an App, set `app.git_branch = Some("main".to_string())`, assert the span contains "main" and has `Color::Cyan` foreground. Tests AC-1. Isolated, fast, deterministic.

`test_search_component_returns_query_in_search_mode`: set `app.search_mode = true` and `app.search_query = "hello".to_string()`, assert span contains "/hello" with `Color::Yellow`. Tests AC-4. Isolated, fast.

`test_search_component_returns_none_when_not_searching`: leave `app.search_mode = false`, assert `search_component(&app).is_none()`. Tests AC-5. Isolated, fast.

`test_type_filter_component_returns_type_in_types_mode`: app defaults to `ViewMode::Types`, assert span contains the current type name. Tests AC-6. Isolated, fast.

`test_type_filter_component_returns_none_in_other_modes`: set `app.view_mode = ViewMode::Graph`, assert `type_filter_component(&app).is_none()`. Tests AC-7. Isolated, fast.

`test_default_components_include_new_components`: assert `StatusBarComponents::default()` has 3 left components and 4 right components.

Unit tests in `src/engine/git_status.rs`:

`test_query_git_branch_in_repo`: run in the project repo, assert returns `Some`. Trades Isolated for Predictive (depends on being in a git repo, same tradeoff as existing tests).

`test_query_git_branch_outside_repo`: use a temp dir, assert returns `None`. Isolated, deterministic.

## Notes

- ACs 8-10 (fullscreen hide/restore, modal overlay) are already satisfied by ITERATION-146 and do not need additional work.
- The git branch icon (U+E0A0) is a Powerline glyph that may not render in all terminals. Using a plain ` ` (branch name only) is safer. The component function should use a simple prefix that degrades gracefully.
- `app.current_type()` indexes into `doc_types` via `selected_type`. In non-Types modes, the index still points at a valid type, so returning it would be misleading. The component should only produce output when `view_mode == ViewMode::Types`.
