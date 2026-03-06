---
title: TUI Test Coverage
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-029-tui-test-coverage.md
---





## Problem

22 of 37 public `App` methods have no test coverage. The untested methods cover search, scrolling, fullscreen, relations navigation, preview tab switching, and the `handle_key` dispatch chain added by ITERATION-014. All methods are directly testable via `App` instances with in-memory Store fixtures.

## Changes

### Task 1: Search method tests

**ACs addressed:** AC-1 (search methods each have at least one test)

**Files:**
- Create: `tests/tui_search_test.rs`

**What to implement:**

Create a new test file following the existing pattern (`mod common; use common::TestFixture;`). Write a `setup_app_with_docs` helper that creates an App with 3 RFCs and 1 Story (enough variety to make search meaningful).

Tests to write:

1. `test_enter_search` - Call `enter_search()`. Assert `search_mode == true`, `search_query` is empty, `search_results` is empty, `search_selected == 0`.

2. `test_exit_search` - Enter search, push some chars to `search_query`, call `update_search()` to populate results. Then call `exit_search()`. Assert `search_mode == false`, `search_query` is empty, `search_results` is empty, `search_selected == 0`.

3. `test_update_search_filters_by_title` - Enter search, set `search_query` to a substring that matches one doc title. Call `update_search()`. Assert `search_results.len() == 1` and the result path matches the expected doc.

4. `test_update_search_empty_query_clears_results` - Enter search, populate results with a query, then clear `search_query` and call `update_search()`. Assert `search_results` is empty.

5. `test_update_search_resets_selected` - Enter search, populate results, set `search_selected = 1`, then change query and call `update_search()`. Assert `search_selected == 0`.

6. `test_select_search_result_navigates_to_doc` - Enter search, set query that matches a Story doc, call `update_search()`. Then call `select_search_result()`. Assert `selected_type` points to the Story type index and `selected_doc` points to that doc. Assert search mode is exited.

7. `test_select_search_result_with_no_results` - Enter search with empty results. Call `select_search_result()`. Assert nothing changes (no panic, type/doc unchanged).

8. `test_search_move_down` - Set up `search_results` with 3 entries, `search_selected = 0`. Call `search_move_down()`. Assert `search_selected == 1`.

9. `test_search_move_down_clamps` - Set up `search_results` with 3 entries, `search_selected = 2`. Call `search_move_down()`. Assert `search_selected == 2`.

10. `test_search_move_up` - `search_selected = 2`. Call `search_move_up()`. Assert `search_selected == 1`.

11. `test_search_move_up_clamps` - `search_selected = 0`. Call `search_move_up()`. Assert `search_selected == 0`.

**How to verify:**
```
cargo test tui_search_test
```

---

### Task 2: Fullscreen and scroll tests

**ACs addressed:** AC-2 (scroll methods each have at least one test covering boundary behavior)

**Files:**
- Create: `tests/tui_fullscreen_test.rs`

**What to implement:**

Create a new test file with a `setup_app_with_docs` helper (same pattern as Task 1, needs at least 1 doc to enter fullscreen).

Tests to write:

1. `test_enter_fullscreen_with_doc` - Select a doc, call `enter_fullscreen()`. Assert `fullscreen_doc == true` and `scroll_offset == 0`.

2. `test_enter_fullscreen_without_doc` - Use empty store (no docs). Call `enter_fullscreen()`. Assert `fullscreen_doc == false` (guard prevents entry).

3. `test_exit_fullscreen` - Enter fullscreen, scroll down a few times, then call `exit_fullscreen()`. Assert `fullscreen_doc == false` and `scroll_offset == 0`.

4. `test_scroll_down` - Enter fullscreen. Call `scroll_down()` 3 times. Assert `scroll_offset == 3`.

5. `test_scroll_up` - Enter fullscreen, set `scroll_offset = 5`. Call `scroll_up()`. Assert `scroll_offset == 4`.

6. `test_scroll_up_clamps_at_zero` - Enter fullscreen, `scroll_offset = 0`. Call `scroll_up()`. Assert `scroll_offset == 0` (saturating sub).

7. `test_move_to_top` - With 3 docs, `selected_doc = 2`. Call `move_to_top()`. Assert `selected_doc == 0`.

8. `test_move_to_bottom` - With 3 docs, `selected_doc = 0`. Call `move_to_bottom()`. Assert `selected_doc == 2`.

9. `test_move_to_bottom_empty` - Empty store. Call `move_to_bottom()`. Assert `selected_doc == 0` (no panic).

**How to verify:**
```
cargo test tui_fullscreen_test
```

---

### Task 3: Relations and preview tab tests

**ACs addressed:** AC-3 (relation methods and toggle_preview_tab each have at least one test)

**Files:**
- Create: `tests/tui_relations_test.rs`

**What to implement:**

Create a new test file. The setup helper needs docs with relationships. Write a `setup_app_with_relations` that creates:
- An RFC (`docs/rfcs/RFC-001-test.md`, title "Test RFC", status "accepted")
- A Story implementing that RFC (`docs/stories/STORY-001-test.md`, title "Test Story", status "draft", implements `docs/rfcs/RFC-001-test.md`)
- An Iteration implementing the Story (`docs/iterations/ITER-001-test.md`, title "Test Iter", status "draft", implements `docs/stories/STORY-001-test.md`)

Use `fixture.write_rfc(...)`, `fixture.write_story(...)`, and `fixture.write_iteration(...)` from `TestFixture`. The Store will pick up the relationships automatically.

Navigate the app to the RFC (type index 0, doc index 0). Switch to Relations tab so relation methods operate on the RFC's relations.

Tests to write:

1. `test_toggle_preview_tab` - Assert `preview_tab == PreviewTab::Preview`. Call `toggle_preview_tab()`. Assert `preview_tab == PreviewTab::Relations` and `selected_relation == 0`.

2. `test_toggle_preview_tab_back` - Toggle twice. Assert back to `PreviewTab::Preview`.

3. `test_toggle_preview_tab_resets_relation` - Set `selected_relation = 1`, then toggle. Assert `selected_relation == 0`.

4. `test_relation_count` - Navigate to the RFC. Assert `relation_count()` equals the number of docs that relate to it (the Story implements it, so at least 1). Navigate to a doc with no relations and assert `relation_count() == 0`.

5. `test_move_relation_down` - Navigate to a doc with 2+ relations. `selected_relation = 0`. Call `move_relation_down()`. Assert `selected_relation == 1`.

6. `test_move_relation_down_clamps` - `selected_relation` at last index. Call `move_relation_down()`. Assert unchanged.

7. `test_move_relation_up` - `selected_relation = 1`. Call `move_relation_up()`. Assert `selected_relation == 0`.

8. `test_move_relation_up_clamps` - `selected_relation = 0`. Call `move_relation_up()`. Assert `selected_relation == 0`.

9. `test_navigate_to_relation` - Navigate to the RFC, switch to Relations tab. The RFC's relation is the Story. Call `navigate_to_relation()`. Assert `selected_type` points to Story type and `selected_doc` points to the Story. Assert `preview_tab` is reset to `Preview` and `selected_relation == 0`.

10. `test_navigate_to_relation_no_doc` - Empty store, call `navigate_to_relation()`. Assert no panic, state unchanged.

> [!NOTE]
> `relation_count` uses `store.related_to()` which returns both forward and reverse relations. The exact count depends on what `related_to` returns for each doc. Tests should assert on the actual returned count rather than hardcoding expected values. Use `app.relation_count()` to get the actual count and validate it's > 0 for docs with known relations.

**How to verify:**
```
cargo test tui_relations_test
```

---

### Task 4: `handle_key` integration tests

**ACs addressed:** AC-4 (integration tests verify key dispatch for each mode)

**Files:**
- Create: `tests/tui_handle_key_test.rs`

**What to implement:**

Create a new test file. These tests exercise `handle_key` directly with `KeyCode` and `KeyModifiers` values, verifying the full dispatch path from key press to state change.

Setup helper: `setup_app_with_docs` that creates an App with at least 2 RFCs (enough to test navigation).

Import `crossterm::event::{KeyCode, KeyModifiers}` for constructing key inputs.

Tests to write:

**Normal mode:**

1. `test_handle_key_quit` - Send `KeyCode::Char('q')` with `KeyModifiers::NONE`. Assert `should_quit == true`.

2. `test_handle_key_ctrl_c_quit` - Send `KeyCode::Char('c')` with `KeyModifiers::CONTROL`. Assert `should_quit == true`.

3. `test_handle_key_help` - Send `KeyCode::Char('?')`. Assert `show_help == true`.

4. `test_handle_key_dismiss_help` - Set `show_help = true`. Send any key. Assert `show_help == false`.

5. `test_handle_key_navigation_j` - Send `KeyCode::Char('j')`. Assert `selected_doc` incremented.

6. `test_handle_key_navigation_k` - Set `selected_doc = 1`. Send `KeyCode::Char('k')`. Assert `selected_doc` decremented.

7. `test_handle_key_type_switch` - Send `KeyCode::Char('l')`. Assert `selected_type` incremented.

8. `test_handle_key_enter_fullscreen` - Send `KeyCode::Enter`. Assert `fullscreen_doc == true`.

9. `test_handle_key_enter_search` - Send `KeyCode::Char('/')`. Assert `search_mode == true`.

10. `test_handle_key_tab_toggles_preview` - Send `KeyCode::Tab`. Assert `preview_tab == PreviewTab::Relations`.

**Search mode:**

11. `test_handle_key_search_esc` - Enter search mode. Send `KeyCode::Esc`. Assert `search_mode == false`.

12. `test_handle_key_search_typing` - Enter search mode. Send `KeyCode::Char('a')`. Assert `search_query == "a"`.

13. `test_handle_key_search_backspace` - Enter search, type "ab". Send `KeyCode::Backspace`. Assert `search_query == "a"`.

14. `test_handle_key_search_ctrl_j` - Enter search, populate results. Send `KeyCode::Char('j')` with `KeyModifiers::CONTROL`. Assert `search_selected` incremented.

**Fullscreen mode:**

15. `test_handle_key_fullscreen_esc` - Enter fullscreen. Send `KeyCode::Esc`. Assert `fullscreen_doc == false`.

16. `test_handle_key_fullscreen_scroll` - Enter fullscreen. Send `KeyCode::Char('j')`. Assert `scroll_offset == 1`.

**Create form mode:**

17. `test_handle_key_create_form_esc` - Open create form. Send `KeyCode::Esc`. Assert `create_form.active == false`.

18. `test_handle_key_create_form_typing` - Open create form. Send `KeyCode::Char('a')`. Assert the active field has "a".

**Delete confirm mode:**

19. `test_handle_key_delete_confirm_esc` - Open delete confirm. Send `KeyCode::Esc`. Assert `delete_confirm.active == false`.

Each test calls `app.handle_key(code, modifiers, fixture.root(), &fixture.config())` -- the full dispatch path.

**How to verify:**
```
cargo test tui_handle_key_test
```

## Test Plan

All new tests. 49 tests across 4 files.

| Test file | Methods covered | Count | Properties |
|-----------|----------------|-------|------------|
| `tui_search_test.rs` | `enter_search`, `exit_search`, `update_search`, `select_search_result`, `search_move_up`, `search_move_down` | 11 | Fast, Isolated, Behavioral, Deterministic, Specific |
| `tui_fullscreen_test.rs` | `enter_fullscreen`, `exit_fullscreen`, `scroll_down`, `scroll_up`, `move_to_top`, `move_to_bottom` | 9 | Fast, Isolated, Behavioral, Deterministic, Specific |
| `tui_relations_test.rs` | `toggle_preview_tab`, `relation_count`, `move_relation_down`, `move_relation_up`, `navigate_to_relation` | 10 | Fast, Isolated, Behavioral, Deterministic |
| `tui_handle_key_test.rs` | `handle_key` (all 5 modes) | 19 | Fast, Isolated, Predictive, Behavioral |

> [!NOTE]
> The `handle_key` tests trade Specific for Predictive. A failure in a handle_key test could be caused by either the dispatch logic or the underlying method. This is acceptable because the unit tests in the other 3 files provide specificity for the individual methods, while the handle_key tests verify the wiring.

## Notes

- Depends on ITERATION-014 (`handle_key` extraction) and ITERATION-015 (`TestFixture`).
- `doc_count` is a trivial getter that delegates to `store.list().len()`. Not worth a dedicated test.
- `current_type`, `docs_for_current_type`, and `selected_doc_meta` are accessors used extensively by the methods under test. They get indirect coverage through every test that checks state after method calls. Dedicated tests would be structure-sensitive without adding behavioral value.
- Task ordering: Tasks 1-3 are independent and can be built in parallel. Task 4 depends on understanding the patterns established in Tasks 1-3 but has no code dependency.
