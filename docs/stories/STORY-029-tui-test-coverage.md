---
title: TUI Test Coverage
type: story
status: accepted
author: agent
date: 2026-03-05
tags: [testing, tui, quality]
related:
- implements: docs/rfcs/RFC-009-codebase-quality-baseline.md
---



## Context

19 of 36 public `App` methods have no test coverage. After ITERATION-014 extracts key handlers into `App` methods, the full key dispatch path becomes testable without a terminal backend.

## Acceptance Criteria

- **Given** `App` has search methods (`enter_search`, `exit_search`, `update_search`, `select_search_result`)
  **When** the test suite runs
  **Then** each method has at least one test covering its primary behavior

- **Given** `App` has scroll methods (`scroll_down`, `scroll_up`, `enter_fullscreen`, `exit_fullscreen`)
  **When** the test suite runs
  **Then** each method has at least one test covering boundary behavior

- **Given** `App` has relation methods (`move_relation_down`, `move_relation_up`, `navigate_to_relation`, `toggle_preview_tab`)
  **When** the test suite runs
  **Then** each method has at least one test covering its primary behavior

- **Given** `App` has a `handle_key` method (added by ITERATION-014)
  **When** the test suite runs
  **Then** integration tests verify key dispatch for each mode (normal, search, fullscreen, create form, delete confirm)

## Scope

### In Scope

- ITERATION-017: TUI Test Coverage

### Out of Scope

- UI rendering tests (would require terminal backend mocking)
- Engine or CLI test coverage gaps
