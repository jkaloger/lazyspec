---
title: TUI Test Coverage
type: iteration
status: draft
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-029-tui-test-coverage.md
---




## Problem

19 of 36 public `App` methods have no test coverage. The untested methods cover search, scrolling, fullscreen, relations navigation, and preview tab switching. After ITERATION-014 extracts key handlers into `App` methods, these become directly testable without a terminal backend.

## Changes

Stub. Full task breakdown to be written when this iteration is picked up. High-level scope:

Untested methods to cover:

- Search: `enter_search`, `exit_search`, `update_search`, `select_search_result`, `search_move_up`, `search_move_down` (last two added by ITERATION-014)
- Fullscreen: `enter_fullscreen`, `exit_fullscreen`, `scroll_down`, `scroll_up`
- Relations: `relation_count`, `move_relation_down`, `move_relation_up`, `navigate_to_relation`
- Preview: `toggle_preview_tab`
- Navigation: `move_to_top`, `move_to_bottom`
- Top-level: `handle_key` (added by ITERATION-014)

## Test Plan

All new tests. Each group should have its own test file or extend the existing `tui_navigation_test.rs`. Tests operate on `App` instances with in-memory Store fixtures (same pattern as existing TUI tests).

## Notes

- Depends on ITERATION-014 (key handler extraction makes `handle_key` testable).
- `handle_key` integration tests are the highest-value addition since they exercise the full dispatch path that was previously only testable via a running terminal.
- `doc_count` is a simple getter with no edge cases worth testing independently.
