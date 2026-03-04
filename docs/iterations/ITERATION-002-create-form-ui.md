---
title: Create Form UI
type: iteration
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- modal
- input
related:
- implements: docs/stories/STORY-006-create-form-ui-and-input-handling.md
---


## Changes

- Add `CreateForm` struct to `tui/app.rs` with form state (active, focused field, field values)
- Add `FormField` enum: Title, Author, Tags, Related
- Add form methods on `App`: open/close form, type chars, backspace, tab/shift-tab navigation
- Add `draw_create_form` to `tui/ui.rs` rendering a centered modal overlay
- Wire `n` keybinding in `tui/mod.rs` event loop to open create form
- Wire Esc, Tab, BackTab, Backspace, and char input in create form mode

## Test Plan

- `test_create_form_opens_with_current_type` (AC1)
- `test_create_form_initial_state` (AC2)
- `test_create_form_text_input` (AC3)
- `test_create_form_backspace` (AC4)
- `test_create_form_tab_navigation` (AC5)
- `test_create_form_shift_tab_navigation` (AC5)
- `test_create_form_cancel` (AC6)

## Notes

AC7 (visual consistency) and AC8 (help text) are rendering concerns tested by visual inspection rather than unit tests. The draw function follows existing overlay patterns.
